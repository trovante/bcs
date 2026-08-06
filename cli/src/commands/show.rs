//! `bcs show` — segment-style path display (RX-inspired DX over decode --path).

use crate::utils;
use anyhow::{Context, Result};
use bcs_core::schema::{
    find_sensitive_plaintext_under, redact_sensitive_plaintext_under,
};
use bcs_core::Decoder;
use std::io::{self, IsTerminal};

pub fn run(
    file: &str,
    segments: &[String],
    format: Option<&str>,
    redact_sensitive_plaintext: bool,
    fail_on_sensitive_plaintext: bool,
) -> Result<()> {
    if !utils::file_exists(file) {
        anyhow::bail!("File not found: {}", file);
    }

    let path = if segments.is_empty() {
        None
    } else {
        Some(segments.join("."))
    };

    let fmt = resolve_format(format);

    let mut decoder =
        Decoder::from_file(file).with_context(|| format!("Failed to load BCS file: {}", file))?;
    let schema = decoder.schema().ok().cloned();

    let mut value = if let Some(ref p) = path {
        decoder
            .get(p)
            .with_context(|| format!("Failed to get path {}", p))?
    } else {
        decoder
            .decode_to_value()
            .context("Failed to decode BCS data")?
    };

    // Always mask secrets for show (no unlock flags on this command).
    bcs_core::security::mask_sensitive_fields(&mut value)
        .map_err(anyhow::Error::msg)
        .context("mask sensitive")?;
    bcs_core::security::mask_secret_refs(&mut value)
        .map_err(anyhow::Error::msg)
        .context("mask secret refs")?;

    if fail_on_sensitive_plaintext || redact_sensitive_plaintext {
        if let Some(schema) = schema.as_ref() {
            let root = path.as_deref();
            if fail_on_sensitive_plaintext {
                let findings = find_sensitive_plaintext_under(schema, &value, root)
                    .map_err(anyhow::Error::msg)
                    .context("Failed to check sensitive plaintext")?;
                if !findings.is_empty() {
                    for f in &findings {
                        eprintln!("error: {}", f.message);
                    }
                    anyhow::bail!(
                        "{} sensitive path(s) hold plaintext; refuse to show (--fail-on-sensitive-plaintext)",
                        findings.len()
                    );
                }
            }
            if redact_sensitive_plaintext {
                let findings = redact_sensitive_plaintext_under(schema, &mut value, root)
                    .map_err(anyhow::Error::msg)
                    .context("Failed to redact sensitive plaintext")?;
                if !findings.is_empty() {
                    eprintln!(
                        "warning: redacted {} sensitive plaintext path(s) to [SENSITIVE]",
                        findings.len()
                    );
                }
            }
        } else {
            utils::print_info(
                "note: --redact-sensitive-plaintext / --fail-on-sensitive-plaintext require an embedded schema; skipping",
            );
        }
    }

    match fmt.as_str() {
        "json" => {
            let json_value = bcs_core::convert::value_to_json(&value)
                .map_err(anyhow::Error::msg)
                .context("convert")?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json_value).context("serialize")?
            );
        }
        "tree" => print_tree_value(&value, "", true),
        other => anyhow::bail!("Unsupported format '{}'. Use tree or json", other),
    }

    Ok(())
}

/// `bcs dump --format debug-tree` — masking-aware inspect AST (same as `inspect --tree`).
pub fn dump_debug_tree(file: &str) -> Result<()> {
    if !utils::file_exists(file) {
        anyhow::bail!("File not found: {}", file);
    }
    let mut decoder =
        Decoder::from_file(file).with_context(|| format!("Failed to load BCS file: {}", file))?;
    let root =
        bcs_core::InspectNode::from_decoder(&mut decoder).context("Failed to build inspect AST")?;
    print!(
        "{}",
        root.format_tree()
            .context("Failed to format inspect tree")?
    );
    Ok(())
}

fn resolve_format(explicit: Option<&str>) -> String {
    if let Some(f) = explicit {
        return f.to_ascii_lowercase();
    }
    if let Ok(f) = std::env::var("BCS_FORMAT") {
        if !f.is_empty() {
            return f.to_ascii_lowercase();
        }
    }
    if io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none() {
        "tree".into()
    } else {
        "json".into()
    }
}

fn print_tree_value(value: &bcs_core::types::Value, indent: &str, last: bool) {
    use bcs_core::types::Value;
    match value {
        Value::Struct(fields) => {
            let count = fields.len();
            for (i, (name, _, child)) in fields.iter().enumerate() {
                let is_last = i + 1 == count;
                let branch = if is_last { "└─ " } else { "├─ " };
                print!("{}{}{}", indent, branch, name);
                match child {
                    Value::Struct(_) | Value::List(_) | Value::Map(_) => {
                        println!();
                        let next = format!("{}{}", indent, if is_last { "   " } else { "│  " });
                        print_tree_value(child, &next, is_last);
                    }
                    leaf => println!(": {}", leaf_string(leaf)),
                }
            }
        }
        Value::List(items) => {
            let count = items.len();
            for (i, child) in items.iter().enumerate() {
                let is_last = i + 1 == count;
                let branch = if is_last { "└─ " } else { "├─ " };
                print!("{}{}[{}]", indent, branch, i);
                match child {
                    Value::Struct(_) | Value::List(_) | Value::Map(_) => {
                        println!();
                        let next = format!("{}{}", indent, if is_last { "   " } else { "│  " });
                        print_tree_value(child, &next, is_last);
                    }
                    leaf => println!(": {}", leaf_string(leaf)),
                }
            }
        }
        Value::Map(entries) => {
            let count = entries.len();
            for (i, (k, child)) in entries.iter().enumerate() {
                let is_last = i + 1 == count;
                let branch = if is_last { "└─ " } else { "├─ " };
                let key = leaf_string(k);
                print!("{}{}{}", indent, branch, key);
                match child {
                    Value::Struct(_) | Value::List(_) | Value::Map(_) => {
                        println!();
                        let next = format!("{}{}", indent, if is_last { "   " } else { "│  " });
                        print_tree_value(child, &next, is_last);
                    }
                    leaf => println!(": {}", leaf_string(leaf)),
                }
            }
        }
        leaf => {
            let _ = last;
            let branch = "└─ ";
            println!("{}{}{}", indent, branch, leaf_string(leaf));
        }
    }
}

fn leaf_string(value: &bcs_core::types::Value) -> String {
    use bcs_core::types::Value;
    match value {
        Value::String(s) => format!("\"{}\"", s),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".into(),
        Value::Int32(v) => v.to_string(),
        Value::Int64(v) => v.to_string(),
        Value::UInt32(v) => v.to_string(),
        Value::UInt64(v) => v.to_string(),
        Value::Float64(v) => v.to_string(),
        other => format!("{:?}", other),
    }
}

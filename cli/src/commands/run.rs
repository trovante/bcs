//! `bcs run` / `bcs env` — decode a BCS file and inject config as process environment.

use crate::utils;
use anyhow::{Context, Result};
use bcs_core::security::KeyWrapper;
use bcs_core::{Decoder, Schema};
use std::collections::BTreeMap;
use std::process::{Command, Stdio};

#[allow(clippy::too_many_arguments)]
pub fn run(
    file: &str,
    command: &[String],
    export_env: bool,
    resolve_secrets: bool,
    secret_provider: Option<&str>,
    password: Option<&str>,
    password_env: Option<&str>,
    unwrap_kms: bool,
    kms_provider: Option<&str>,
    dry_run: bool,
    json_env: Option<&str>,
    path: Option<&str>,
    prefix: Option<&str>,
    only: Option<&str>,
) -> Result<()> {
    if !utils::file_exists(file) {
        anyhow::bail!("BCS file not found: {}", file);
    }
    let mut argv: Vec<String> = command.to_vec();
    if argv.first().map(|s| s.as_str()) == Some("--") {
        argv.remove(0);
    }
    if argv.is_empty() && !dry_run {
        anyhow::bail!("Provide a command after `--`, or use --dry-run / bcs env");
    }

    let prepared = prepare_env(
        file,
        resolve_secrets,
        secret_provider,
        password,
        password_env,
        unwrap_kms,
        kms_provider,
        path,
        prefix,
        only,
    )?;

    if dry_run {
        utils::print_info("Dry-run env keys (sensitive values redacted):");
        print_env_lines(&prepared.flat, &prepared.sensitive, true, false);
        if let Some(var) = json_env {
            let name = apply_prefix_to_key(var, prefix);
            println!("{}=[REDACTED_JSON]", name);
        }
        return Ok(());
    }

    let mut child = Command::new(&argv[0]);
    child.args(&argv[1..]);
    child.stdin(Stdio::inherit());
    child.stdout(Stdio::inherit());
    child.stderr(Stdio::inherit());

    let should_flatten = export_env || json_env.is_none();
    if should_flatten {
        for (key, val) in &prepared.flat {
            child.env(key, val);
        }
    }

    if let Some(var) = json_env {
        let json_value = bcs_core::convert::value_to_json(&prepared.value)
            .map_err(anyhow::Error::msg)
            .context("convert to JSON for env")?;
        let json = serde_json::to_string(&json_value).context("serialize JSON env")?;
        let name = apply_prefix_to_key(var, prefix);
        child.env(name, json);
    }

    let status = child
        .status()
        .with_context(|| format!("Failed to spawn {:?}", argv))?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

/// Print `KEY='value'` lines for shell `eval` (redacts sensitive unless `allow_sensitive`).
#[allow(clippy::too_many_arguments)]
pub fn env_print(
    file: &str,
    resolve_secrets: bool,
    secret_provider: Option<&str>,
    password: Option<&str>,
    password_env: Option<&str>,
    unwrap_kms: bool,
    kms_provider: Option<&str>,
    path: Option<&str>,
    prefix: Option<&str>,
    only: Option<&str>,
    allow_sensitive: bool,
) -> Result<()> {
    if !utils::file_exists(file) {
        anyhow::bail!("BCS file not found: {}", file);
    }

    let prepared = prepare_env(
        file,
        resolve_secrets,
        secret_provider,
        password,
        password_env,
        unwrap_kms,
        kms_provider,
        path,
        prefix,
        only,
    )?;

    if allow_sensitive {
        eprintln!(
            "warning: --allow-sensitive prints secret values to stdout; prefer `bcs run` for child injection"
        );
    }

    print_env_lines(&prepared.flat, &prepared.sensitive, !allow_sensitive, true);
    Ok(())
}

struct PreparedEnv {
    value: bcs_core::types::Value,
    flat: BTreeMap<String, String>,
    sensitive: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
fn prepare_env(
    file: &str,
    resolve_secrets: bool,
    secret_provider: Option<&str>,
    password: Option<&str>,
    password_env: Option<&str>,
    unwrap_kms: bool,
    kms_provider: Option<&str>,
    path: Option<&str>,
    prefix: Option<&str>,
    only: Option<&str>,
) -> Result<PreparedEnv> {
    let mut decoder =
        Decoder::from_file(file).with_context(|| format!("Failed to load BCS file: {}", file))?;
    let schema = decoder.schema().ok().cloned();

    let mut value = if let Some(p) = path {
        decoder
            .get(p)
            .with_context(|| format!("Failed to get path {}", p))?
    } else {
        decoder
            .decode_to_value()
            .context("Failed to decode BCS data")?
    };

    if password.is_some() {
        crate::utils::warn_password_on_argv("--password", "--password-env");
    }

    let resolved_password = resolve_password(password, password_env)?;
    let kms_wrapper = if unwrap_kms {
        Some(crate::kms_wrapper::resolve_unwrap_wrapper(kms_provider)?)
    } else {
        None
    };
    let wrapper = kms_wrapper.as_ref().map(|w| w.as_ref() as &dyn KeyWrapper);

    if resolved_password.is_some() || wrapper.is_some() {
        bcs_core::security::reveal_all_ex(
            &mut value,
            resolved_password.as_deref(),
            wrapper,
        )
        .map_err(anyhow::Error::msg)
        .context("Failed to reveal protected fields")?;
    }

    if resolve_secrets {
        let resolver = crate::commands::decode::build_secret_resolver(secret_provider)?;
        bcs_core::security::resolve_secret_refs(&mut value, &resolver)
            .map_err(anyhow::Error::msg)
            .context("Failed to resolve secret refs")?;
    }

    let mut flat = flatten_env(&value, "");
    if let Some(only_spec) = only {
        flat = filter_only(flat, only_spec);
    }
    if let Some(p) = prefix {
        flat = apply_prefix(flat, p);
    }

    let sensitive = schema
        .as_ref()
        .map(Schema::sensitive_path_list)
        .unwrap_or_default();

    Ok(PreparedEnv {
        value,
        flat,
        sensitive,
    })
}

fn print_env_lines(
    flat: &BTreeMap<String, String>,
    sensitive: &[String],
    redact: bool,
    shell_quote: bool,
) {
    for (key, val) in flat {
        let is_sensitive = redact && is_sensitive_entry(key, sensitive);
        let display = if is_sensitive {
            "[REDACTED]"
        } else {
            val.as_str()
        };
        if shell_quote {
            println!("{}={}", key, shell_single_quote(display));
        } else {
            println!("{}={}", key, display);
        }
    }
}

fn is_sensitive_entry(key: &str, sensitive: &[String]) -> bool {
    if looks_sensitive_key(key) {
        return true;
    }
    let dotted = env_key_to_path(key);
    if sensitive
        .iter()
        .any(|p| p == &dotted || dotted.starts_with(&format!("{}.", p)))
    {
        return true;
    }
    // Prefixed keys: APP_DATABASE__PASSWORD → match schema path after stripping prefix segment
    let parts: Vec<&str> = key.split("__").collect();
    for start in 1..parts.len() {
        let suffix = parts[start..].join("__");
        let dotted_suffix = env_key_to_path(&suffix);
        if sensitive
            .iter()
            .any(|p| p == &dotted_suffix || dotted_suffix.starts_with(&format!("{}.", p)))
        {
            return true;
        }
        if start == 1 {
            if let Some((_, rest)) = parts[0].split_once('_') {
                if !rest.is_empty() {
                    let mut alt = vec![rest];
                    alt.extend_from_slice(&parts[1..]);
                    let alt_key = alt.join("__");
                    let dotted_alt = env_key_to_path(&alt_key);
                    if sensitive
                        .iter()
                        .any(|p| p == &dotted_alt || dotted_alt.starts_with(&format!("{}.", p)))
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn shell_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn apply_prefix(flat: BTreeMap<String, String>, prefix: &str) -> BTreeMap<String, String> {
    flat.into_iter()
        .map(|(k, v)| (format!("{}{}", prefix, k), v))
        .collect()
}

fn apply_prefix_to_key(key: &str, prefix: Option<&str>) -> String {
    match prefix {
        Some(p) => format!("{}{}", p, key),
        None => key.to_string(),
    }
}

fn filter_only(flat: BTreeMap<String, String>, only_spec: &str) -> BTreeMap<String, String> {
    let paths: Vec<String> = only_spec
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if paths.is_empty() {
        return flat;
    }
    flat.into_iter()
        .filter(|(key, _)| {
            let dotted = env_key_to_path(key);
            paths
                .iter()
                .any(|p| dotted == *p || dotted.starts_with(&format!("{}.", p)))
        })
        .collect()
}

fn resolve_password(direct: Option<&str>, env_var: Option<&str>) -> Result<Option<String>> {
    if let Some(password) = direct {
        return Ok(Some(password.to_string()));
    }
    if let Some(var_name) = env_var {
        let value = std::env::var(var_name)
            .with_context(|| format!("Failed to read password from env var: {}", var_name))?;
        if value.is_empty() {
            anyhow::bail!("Environment variable '{}' is empty", var_name);
        }
        return Ok(Some(value));
    }
    Ok(None)
}

fn flatten_env(value: &bcs_core::types::Value, prefix: &str) -> BTreeMap<String, String> {
    use bcs_core::types::Value;
    let mut out = BTreeMap::new();
    match value {
        Value::Struct(fields) => {
            for (name, _, child) in fields {
                let key = if prefix.is_empty() {
                    sanitize_env_key(name)
                } else {
                    format!("{}__{}", prefix, sanitize_env_key(name))
                };
                merge_flat(&mut out, flatten_env(child, &key));
            }
        }
        Value::Map(entries) => {
            for (k, child) in entries {
                let name = match k {
                    Value::String(s) => sanitize_env_key(s),
                    _ => continue,
                };
                let key = if prefix.is_empty() {
                    name
                } else {
                    format!("{}__{}", prefix, name)
                };
                merge_flat(&mut out, flatten_env(child, &key));
            }
        }
        Value::List(items) => {
            for (i, child) in items.iter().enumerate() {
                let key = if prefix.is_empty() {
                    i.to_string()
                } else {
                    format!("{}__{}", prefix, i)
                };
                merge_flat(&mut out, flatten_env(child, &key));
            }
        }
        Value::Optional(Some(inner)) => merge_flat(&mut out, flatten_env(inner, prefix)),
        Value::Null => {}
        other => {
            if !prefix.is_empty() {
                out.insert(prefix.to_string(), value_to_env_string(other));
            }
        }
    }
    out
}

fn merge_flat(dst: &mut BTreeMap<String, String>, src: BTreeMap<String, String>) {
    dst.extend(src);
}

fn sanitize_env_key(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn env_key_to_path(key: &str) -> String {
    key.split("__")
        .map(|s| s.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(".")
}

fn looks_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.contains("password")
        || lower.contains("secret")
        || lower.contains("token")
        || lower.ends_with("_key")
}

fn value_to_env_string(value: &bcs_core::types::Value) -> String {
    use bcs_core::types::Value;
    match value {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Int8(v) => v.to_string(),
        Value::Int16(v) => v.to_string(),
        Value::Int32(v) => v.to_string(),
        Value::Int64(v) => v.to_string(),
        Value::UInt8(v) => v.to_string(),
        Value::UInt16(v) => v.to_string(),
        Value::UInt32(v) => v.to_string(),
        Value::UInt64(v) => v.to_string(),
        Value::Float32(v) => v.to_string(),
        Value::Float64(v) => v.to_string(),
        Value::Bytes(b) => base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b),
        other => format!("{:?}", other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bcs_core::types::Value;

    #[test]
    fn flatten_nested_uses_double_underscore() {
        let value = Value::Struct(vec![(
            "database".into(),
            0,
            Value::Struct(vec![
                ("host".into(), 0, Value::String("db.local".into())),
                ("port".into(), 0, Value::Int32(5432)),
            ]),
        )]);
        let flat = flatten_env(&value, "");
        assert_eq!(flat.get("DATABASE__HOST").map(String::as_str), Some("db.local"));
        assert_eq!(flat.get("DATABASE__PORT").map(String::as_str), Some("5432"));
    }

    #[test]
    fn prefix_and_only_filter() {
        let mut flat = BTreeMap::new();
        flat.insert("DATABASE__HOST".into(), "h".into());
        flat.insert("DATABASE__PASSWORD".into(), "secret".into());
        flat.insert("API__PORT".into(), "80".into());
        let filtered = filter_only(flat, "database.host,database.password");
        assert_eq!(filtered.len(), 2);
        let prefixed = apply_prefix(filtered, "APP_");
        assert!(prefixed.contains_key("APP_DATABASE__HOST"));
        assert!(prefixed.contains_key("APP_DATABASE__PASSWORD"));
        assert!(!prefixed.contains_key("APP_API__PORT"));
    }

    #[test]
    fn sensitive_match_with_prefix() {
        let sensitive = vec!["database.password".to_string()];
        assert!(is_sensitive_entry("APP_DATABASE__PASSWORD", &sensitive));
        assert!(!is_sensitive_entry("APP_DATABASE__HOST", &sensitive));
    }

    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_single_quote("a'b"), "'a'\\''b'");
    }
}

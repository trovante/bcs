// Validate command implementation

use crate::utils;
use anyhow::{Context, Result};
use bcs_core::{find_sensitive_plaintext, Decoder, SchemaEngine};
use serde_json::json;

pub fn run(file: &str, json_output: bool, fail_on_sensitive_plaintext: bool) -> Result<()> {
    if !utils::file_exists(file) {
        anyhow::bail!("File not found: {}", file);
    }

    if !json_output {
        utils::print_info(&format!("Validating BCS file: {}", file));
    }

    let mut decoder =
        Decoder::from_file(file).with_context(|| format!("Failed to load BCS file: {}", file))?;

    let schema = decoder
        .schema()
        .context("Failed to extract schema from BCS file")?
        .clone();

    if !json_output {
        utils::print_info("Schema loaded successfully");
    }

    let value = decoder
        .decode_to_value()
        .context("Failed to decode BCS data")?;

    if !json_output {
        utils::print_info("Data decoded successfully");
    }

    let engine = SchemaEngine::new();
    let validation_result = engine.validate(&value, &schema);
    let sensitive_findings = find_sensitive_plaintext(&schema, &value)
        .context("Failed to check sensitive plaintext policy")?;

    if json_output {
        let errors: Vec<serde_json::Value> = validation_result
            .errors
            .iter()
            .map(|e| {
                json!({
                    "path": if e.path.is_empty() { "<root>" } else { &e.path },
                    "message": e.message,
                    "kind": "schema"
                })
            })
            .collect();

        let warnings: Vec<serde_json::Value> = sensitive_findings
            .iter()
            .map(|f| {
                json!({
                    "path": f.path,
                    "message": f.message,
                    "kind": "sensitive_plaintext"
                })
            })
            .collect();

        let schema_ok = validation_result.is_valid();
        let sensitive_fail = fail_on_sensitive_plaintext && !sensitive_findings.is_empty();
        let ok = schema_ok && !sensitive_fail;

        let payload = json!({
            "ok": ok,
            "error_count": validation_result.errors.len()
                + if sensitive_fail { sensitive_findings.len() } else { 0 },
            "warning_count": if fail_on_sensitive_plaintext {
                0
            } else {
                sensitive_findings.len()
            },
            "errors": errors,
            "warnings": warnings,
            "fail_on_sensitive_plaintext": fail_on_sensitive_plaintext
        });

        println!(
            "{}",
            serde_json::to_string_pretty(&payload).context("Failed to serialize validate JSON")?
        );

        if ok {
            return Ok(());
        }

        anyhow::bail!("Validation failed");
    }

    for finding in &sensitive_findings {
        utils::print_warning(&finding.message);
    }

    if validation_result.is_valid() {
        if fail_on_sensitive_plaintext && !sensitive_findings.is_empty() {
            anyhow::bail!(
                "Validation failed: {} sensitive path(s) hold plaintext (use protect or secret refs)",
                sensitive_findings.len()
            );
        }
        if sensitive_findings.is_empty() {
            utils::print_success("Validation passed! No errors found.");
        } else {
            utils::print_success(&format!(
                "Validation passed with {} sensitive-plaintext warning(s). Use --fail-on-sensitive-plaintext to fail.",
                sensitive_findings.len()
            ));
        }
        Ok(())
    } else {
        eprintln!(
            "\n❌ Validation failed with {} error(s):\n",
            validation_result.errors.len()
        );

        for (i, error) in validation_result.errors.iter().enumerate() {
            eprintln!(
                "  {}. Path: {}",
                i + 1,
                if error.path.is_empty() {
                    "<root>"
                } else {
                    &error.path
                }
            );
            eprintln!("     Error: {}\n", error.message);
        }

        anyhow::bail!("Validation failed");
    }
}

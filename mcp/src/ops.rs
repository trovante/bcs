//! Pure BCS operations used by MCP tools (no MCP protocol).

use bcs_core::convert::value_to_json;
use bcs_core::security::mask_sensitive_fields;
use bcs_core::{find_sensitive_plaintext, scan_path, Decoder, ScanFailOn, SchemaEngine};
use serde_json::{json, Value as JsonValue};
use std::path::Path;

pub type OpsResult<T> = Result<T, String>;

fn err(e: impl ToString) -> String {
    e.to_string()
}

/// Agent-safe schema JSON for a `.bcs` file (never includes data values).
pub fn schema_agent_safe(path: &Path) -> OpsResult<String> {
    let mut decoder = Decoder::from_file(path).map_err(err)?;
    let schema = decoder.schema().map_err(err)?.clone();
    schema.to_agent_safe_json().map_err(err)
}

/// Header / schema / index metadata without data values.
pub fn inspect_meta(path: &Path) -> OpsResult<JsonValue> {
    let mut decoder = Decoder::from_file(path).map_err(err)?;
    let metadata = decoder.metadata();
    let header = decoder.header().clone();

    let schema_json = match decoder.schema() {
        Ok(schema) => {
            let sensitive = schema.sensitive_path_list();
            json!({
                "ok": true,
                "version": schema.version,
                "root": schema.root,
                "type_count": schema.types.len(),
                "constraint_count": schema.constraints.len(),
                "documentation_count": schema.documentation.len(),
                "sensitive_count": sensitive.len(),
                "sensitive_paths": sensitive,
            })
        }
        Err(e) => json!({ "ok": false, "error": e.to_string() }),
    };

    let index_json = match decoder.index_table() {
        Ok(idx) => json!({
            "ok": true,
            "entry_count": idx.entry_count(),
            "bucket_count": idx.bucket_count(),
            "load_factor": idx.load_factor(),
            "collision_rate": idx.collision_rate(),
        }),
        Err(e) => json!({ "ok": false, "error": e.to_string() }),
    };

    Ok(json!({
        "file": path.display().to_string(),
        "metadata": {
            "version_major": metadata.version_major,
            "version_minor": metadata.version_minor,
            "compressed": metadata.compressed,
            "data_compressed": header.flags.data_compressed,
            "structural_dedup": header.flags.structural_dedup,
            "semantic_size": metadata.semantic_size,
            "index_size": metadata.index_size,
            "data_size": metadata.data_size,
            "total_size": metadata.total_size,
        },
        "header": {
            "flags": header.flags.to_u16(),
            "checksum": header.checksum,
            "semantic_offset": header.semantic_offset,
            "index_offset": header.index_offset,
            "data_offset": header.data_offset,
        },
        "schema": schema_json,
        "index_table": index_json,
    }))
}

/// Validate schema + sensitive-plaintext policy (JSON report).
pub fn validate(path: &Path, fail_on_sensitive_plaintext: bool) -> OpsResult<JsonValue> {
    let mut decoder = Decoder::from_file(path).map_err(err)?;
    let schema = decoder.schema().map_err(err)?.clone();
    let value = decoder.decode_to_value().map_err(err)?;

    let engine = SchemaEngine::new();
    let validation_result = engine.validate(&value, &schema);
    let sensitive_findings = find_sensitive_plaintext(&schema, &value).map_err(err)?;

    let errors: Vec<JsonValue> = validation_result
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

    let warnings: Vec<JsonValue> = sensitive_findings
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

    Ok(json!({
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
        "fail_on_sensitive_plaintext": fail_on_sensitive_plaintext,
    }))
}

/// Scan file or directory; returns the same JSON shape as `bcs scan --json`.
pub fn scan(path: &Path, fail_on: &str) -> OpsResult<JsonValue> {
    let fail = ScanFailOn::parse(fail_on).map_err(err)?;
    let report = scan_path(path, fail).map_err(err)?;
    serde_json::to_value(&report).map_err(|e| e.to_string())
}

/// Path get with protect / secret-ref masking. Never accepts a password.
pub fn get_path_masked(bcs_path: &Path, query: &str) -> OpsResult<JsonValue> {
    let mut decoder = Decoder::from_file(bcs_path).map_err(err)?;
    let mut value = decoder.get(query).map_err(err)?;
    mask_sensitive_fields(&mut value).map_err(err)?;
    value_to_json(&value).map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bcs_core::security::protect_paths;
    use bcs_core::{Decoder, Encoder};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn schema_has_no_data_values() {
        let dir = tempdir().unwrap();
        let bcs_path = dir.path().join("cfg.bcs");
        let mut enc = Encoder::new();
        let bytes = enc
            .encode_from_json(r#"{"host":"localhost","password":"s3cret"}"#)
            .unwrap();
        fs::write(&bcs_path, bytes).unwrap();

        let out = schema_agent_safe(&bcs_path).unwrap();
        assert!(!out.contains("s3cret"));
        assert!(!out.contains("localhost"));
    }

    #[test]
    fn get_path_masks_protected_marker() {
        let dir = tempdir().unwrap();
        let bcs_path = dir.path().join("sec.bcs");

        let mut enc = Encoder::new();
        let plain = enc
            .encode_from_json(r#"{"host":"db.example","password":"hunter2"}"#)
            .unwrap();
        let mut dec = Decoder::from_bytes(&plain).unwrap();
        let mut value = dec.decode_to_value().unwrap();
        protect_paths(&mut value, &["password".to_string()], "test-password").unwrap();
        let json = serde_json::to_string(&value_to_json(&value).unwrap()).unwrap();
        let mut enc2 = Encoder::new();
        let bytes = enc2.encode_from_json(&json).unwrap();
        fs::write(&bcs_path, bytes).unwrap();

        let got = get_path_masked(&bcs_path, "password").unwrap();
        let s = got.to_string();
        assert!(!s.contains("hunter2"), "must not leak plaintext: {}", s);
        assert!(s.contains("[PROTECTED]"), "expected mask, got {}", s);
    }

    #[test]
    fn scan_detects_aws_key_in_json() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("leak.json");
        fs::write(&path, r#"{"key":"AKIAIOSFODNN7EXAMPLE"}"#).unwrap();
        let report = scan(&path, "finding").unwrap();
        assert_eq!(report["ok"], false);
        assert!(report["finding_count"].as_u64().unwrap() >= 1);
    }

    #[test]
    fn inspect_meta_lists_file() {
        let dir = tempdir().unwrap();
        let bcs_path = dir.path().join("cfg.bcs");
        let mut enc = Encoder::new();
        fs::write(
            &bcs_path,
            enc.encode_from_json(r#"{"a":1}"#).unwrap(),
        )
        .unwrap();
        let meta = inspect_meta(&bcs_path).unwrap();
        assert!(meta["metadata"]["total_size"].as_u64().unwrap() > 0);
        assert_eq!(meta["schema"]["ok"], true);
    }

    #[test]
    fn validate_ok_for_simple_file() {
        let dir = tempdir().unwrap();
        let bcs_path = dir.path().join("cfg.bcs");
        let mut enc = Encoder::new();
        fs::write(
            &bcs_path,
            enc.encode_from_json(r#"{"port":8080}"#).unwrap(),
        )
        .unwrap();
        let report = validate(&bcs_path, false).unwrap();
        assert_eq!(report["ok"], true);
    }
}

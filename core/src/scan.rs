//! Leak / sensitive-plaintext scanner for sources and `.bcs` files.
//!
//! Shared by the CLI (`bcs scan`) and the MCP server (`bcs_scan`).

use crate::error::{BCSError, Result};
use crate::schema::find_sensitive_plaintext;
use crate::security::{is_protected_marker, is_secret_ref_marker};
use crate::types::Value;
use crate::Decoder;
use regex::Regex;
use serde::Serialize;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

/// Fail policy for scan reports (mirrors CLI `--fail-on`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanFailOn {
    #[default]
    Finding,
    Warn,
}

impl ScanFailOn {
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "finding" | "findings" => Ok(Self::Finding),
            "warn" | "warning" | "warnings" => Ok(Self::Warn),
            other => Err(BCSError::Decoding(format!(
                "Invalid scan fail_on '{}'. Use finding or warn",
                other
            ))),
        }
    }
}

/// One scan finding or warning.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ScanFinding {
    pub severity: &'static str,
    pub kind: &'static str,
    pub path: String,
    pub location: String,
    pub message: String,
}

/// Structured scan report (same shape as `bcs scan --json`).
#[derive(Debug, Clone, Serialize)]
pub struct ScanReport {
    pub ok: bool,
    pub finding_count: usize,
    pub warning_count: usize,
    pub fail_on: ScanFailOn,
    pub findings: Vec<ScanFinding>,
}

impl ScanReport {
    pub fn from_findings(findings: Vec<ScanFinding>, fail_on: ScanFailOn) -> Self {
        let finding_count = findings.iter().filter(|f| f.severity == "finding").count();
        let warning_count = findings.iter().filter(|f| f.severity == "warn").count();
        let should_fail = match fail_on {
            ScanFailOn::Finding => finding_count > 0,
            ScanFailOn::Warn => finding_count > 0 || warning_count > 0,
        };
        Self {
            ok: !should_fail,
            finding_count,
            warning_count,
            fail_on,
            findings,
        }
    }

    pub fn to_json_pretty(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| BCSError::Decoding(format!("Failed to serialize scan report: {}", e)))
    }
}

/// Scan a file or directory for secret patterns and sensitive plaintext.
pub fn scan_path(path: &Path, fail_on: ScanFailOn) -> Result<ScanReport> {
    if !path.exists() {
        return Err(BCSError::Decoding(format!(
            "Path not found: {}",
            path.display()
        )));
    }
    let mut findings = Vec::new();
    if path.is_dir() {
        scan_dir(path, &mut findings)?;
    } else {
        scan_file(path, &mut findings)?;
    }
    Ok(ScanReport::from_findings(findings, fail_on))
}

fn scan_dir(dir: &Path, findings: &mut Vec<ScanFinding>) -> Result<()> {
    let entries = fs::read_dir(dir)
        .map_err(|e| BCSError::Decoding(format!("read dir {}: {}", dir.display(), e)))?;
    for entry in entries {
        let entry = entry.map_err(|e| BCSError::Decoding(format!("read dir entry: {}", e)))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|e| BCSError::Decoding(format!("file type {}: {}", path.display(), e)))?;
        // Do not follow symlinks (prevents escaping the intended scan root).
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if matches!(name, "target" | ".git" | "node_modules") {
                continue;
            }
            scan_dir(&path, findings)?;
        } else if file_type.is_file() {
            scan_file(&path, findings)?;
        }
    }
    Ok(())
}

fn scan_file(path: &Path, findings: &mut Vec<ScanFinding>) -> Result<()> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "json" | "yaml" | "yml" | "toml" => scan_text_source(path, findings)?,
        "bcs" => scan_bcs(path, findings)?,
        _ => {}
    }
    Ok(())
}

fn scan_text_source(path: &Path, findings: &mut Vec<ScanFinding>) -> Result<()> {
    let content = fs::read_to_string(path)
        .map_err(|e| BCSError::Decoding(format!("read {}: {}", path.display(), e)))?;
    let location = path.display().to_string();

    for (kind, re) in secret_patterns() {
        for mat in re.find_iter(&content) {
            let snippet = mat.as_str();
            if snippet.contains("__bcs_sensitive_") || snippet.contains("__bcs_secret_ref__") {
                continue;
            }
            findings.push(ScanFinding {
                severity: "finding",
                kind,
                path: location.clone(),
                location: format!("{}:offset {}", location, mat.start()),
                message: format!("Possible secret pattern matched ({})", kind),
            });
        }
    }

    if let Ok(value) = parse_source_to_json(&content, &ext_of(path)) {
        walk_json_secrets(&value, "", &location, findings);
    }
    Ok(())
}

fn scan_bcs(path: &Path, findings: &mut Vec<ScanFinding>) -> Result<()> {
    let location = path.display().to_string();
    let mut decoder = Decoder::from_file(path)
        .map_err(|e| BCSError::Decoding(format!("open BCS {}: {}", location, e)))?;
    let schema = decoder.schema().ok().cloned();
    let value = decoder
        .decode_to_value()
        .map_err(|e| BCSError::Decoding(format!("decode BCS {}: {}", location, e)))?;

    if let Some(schema) = schema {
        let plaintext = find_sensitive_plaintext(&schema, &value)?;
        for f in plaintext {
            findings.push(ScanFinding {
                severity: "finding",
                kind: "sensitive_plaintext",
                path: f.path.clone(),
                location: format!("{}:{}", location, f.path),
                message: f.message,
            });
        }
    }

    walk_value_secrets(&value, "", &location, findings)?;
    Ok(())
}

fn walk_value_secrets(
    value: &Value,
    path: &str,
    file: &str,
    findings: &mut Vec<ScanFinding>,
) -> Result<()> {
    match value {
        Value::String(s) => {
            if is_protected_marker(value) || is_secret_ref_marker(value) {
                return Ok(());
            }
            check_string_patterns(s, path, file, findings);
            let leaf = path.rsplit('.').next().unwrap_or(path).to_ascii_lowercase();
            let leaf = leaf
                .trim_end_matches(']')
                .rsplit('[')
                .next()
                .unwrap_or(&leaf);
            if looks_like_secret_key(leaf) && s.len() >= 8 {
                findings.push(ScanFinding {
                    severity: "warn",
                    kind: "unmarked_secret_intent",
                    path: path.to_string(),
                    location: format!("{}:{}", file, path),
                    message: format!(
                        "Path '{}' looks secret-related but is not protect/sensitive marked",
                        path
                    ),
                });
            }
        }
        Value::Struct(fields) => {
            for (name, _, child) in fields {
                let child_path = if path.is_empty() {
                    name.clone()
                } else {
                    format!("{}.{}", path, name)
                };
                walk_value_secrets(child, &child_path, file, findings)?;
            }
        }
        Value::List(items) => {
            for (i, child) in items.iter().enumerate() {
                walk_value_secrets(child, &format!("{}[{}]", path, i), file, findings)?;
            }
        }
        Value::Map(entries) => {
            for (i, (k, child)) in entries.iter().enumerate() {
                let key = if let Value::String(s) = k {
                    s.clone()
                } else {
                    format!("{}", i)
                };
                let child_path = if path.is_empty() {
                    key
                } else {
                    format!("{}.{}", path, key)
                };
                walk_value_secrets(child, &child_path, file, findings)?;
            }
        }
        Value::Optional(Some(inner)) => walk_value_secrets(inner, path, file, findings)?,
        _ => {}
    }
    Ok(())
}

fn walk_json_secrets(
    value: &serde_json::Value,
    path: &str,
    file: &str,
    findings: &mut Vec<ScanFinding>,
) {
    match value {
        serde_json::Value::String(s) => {
            if s.starts_with("__bcs_sensitive_") || s.starts_with("__bcs_secret_ref__") {
                return;
            }
            check_string_patterns(s, path, file, findings);
            let leaf = path.rsplit('.').next().unwrap_or(path).to_ascii_lowercase();
            if looks_like_secret_key(&leaf) && s.len() >= 8 {
                findings.push(ScanFinding {
                    severity: "warn",
                    kind: "unmarked_secret_intent",
                    path: path.to_string(),
                    location: format!("{}:{}", file, path),
                    message: format!(
                        "Path '{}' looks secret-related but is not protect/sensitive marked",
                        path
                    ),
                });
            }
        }
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let child = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{}.{}", path, k)
                };
                walk_json_secrets(v, &child, file, findings);
            }
        }
        serde_json::Value::Array(arr) => {
            for (i, v) in arr.iter().enumerate() {
                walk_json_secrets(v, &format!("{}[{}]", path, i), file, findings);
            }
        }
        _ => {}
    }
}

fn check_string_patterns(s: &str, path: &str, file: &str, findings: &mut Vec<ScanFinding>) {
    for (kind, re) in secret_patterns() {
        if re.is_match(s) {
            findings.push(ScanFinding {
                severity: "finding",
                kind,
                path: path.to_string(),
                location: format!("{}:{}", file, path),
                message: format!("Possible secret pattern matched ({})", kind),
            });
        }
    }
}

fn looks_like_secret_key(name: &str) -> bool {
    matches!(
        name,
        "password"
            | "passwd"
            | "secret"
            | "token"
            | "api_key"
            | "apikey"
            | "access_key"
            | "private_key"
            | "client_secret"
    ) || name.contains("password")
        || name.contains("secret")
        || name.ends_with("_key")
        || name.ends_with("_token")
}

fn secret_patterns() -> &'static [(&'static str, Regex)] {
    static PATTERNS: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        vec![
            (
                "aws_access_key_id",
                Regex::new(r"(?i)\bAKIA[0-9A-Z]{16}\b").unwrap(),
            ),
            (
                "pem_private_key",
                Regex::new(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----").unwrap(),
            ),
            (
                "github_pat",
                Regex::new(r"\bghp_[A-Za-z0-9]{36,}\b").unwrap(),
            ),
            (
                "slack_token",
                Regex::new(r"\bxox[baprs]-[A-Za-z0-9-]{10,}\b").unwrap(),
            ),
            (
                "generic_bearer",
                Regex::new(r"(?i)\bBearer\s+[A-Za-z0-9\-._~+/]+=*\b").unwrap(),
            ),
        ]
    })
}

fn ext_of(path: &Path) -> String {
    path.extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn parse_source_to_json(content: &str, ext: &str) -> Result<serde_json::Value> {
    match ext {
        "json" => serde_json::from_str(content)
            .map_err(|e| BCSError::Decoding(format!("JSON parse: {}", e))),
        "yaml" | "yml" => serde_yaml::from_str(content)
            .map_err(|e| BCSError::Decoding(format!("YAML parse: {}", e))),
        "toml" => {
            let v: toml::Value = toml::from_str(content)
                .map_err(|e| BCSError::Decoding(format!("TOML parse: {}", e)))?;
            serde_json::to_value(v).map_err(|e| BCSError::Decoding(format!("TOML to JSON: {}", e)))
        }
        _ => Err(BCSError::Decoding("unsupported source format".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn scan_json_detects_aws_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("leak.json");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, r#"{{"key":"AKIAIOSFODNN7EXAMPLE"}}"#).unwrap();
        let report = scan_path(&path, ScanFailOn::Finding).unwrap();
        assert!(!report.ok);
        assert!(report
            .findings
            .iter()
            .any(|f| f.kind == "aws_access_key_id"));
    }

    #[test]
    fn scan_skips_protected_markers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ok.json");
        std::fs::write(
            &path,
            r#"{"database":{"password":"__bcs_sensitive_pbkdf2__:aabbcc"}}"#,
        )
        .unwrap();
        let report = scan_path(&path, ScanFailOn::Finding).unwrap();
        assert!(
            report
                .findings
                .iter()
                .filter(|f| f.severity == "finding")
                .count()
                == 0,
            "{:?}",
            report.findings
        );
    }

    #[test]
    fn scan_dir_skips_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let leak = outside.path().join("leak.json");
        std::fs::write(&leak, r#"{"key":"AKIAIOSFODNN7EXAMPLE"}"#).unwrap();
        let link = dir.path().join("escape");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outside.path(), &link).unwrap();
            let report = scan_path(dir.path(), ScanFailOn::Finding).unwrap();
            assert!(
                report.findings.is_empty(),
                "symlink escape should be skipped: {:?}",
                report.findings
            );
        }
        #[cfg(not(unix))]
        {
            let _ = (outside, link, dir);
        }
    }

    #[test]
    fn scan_warns_unmarked_password_field() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("warn.json");
        std::fs::write(&path, r#"{"database":{"password":"supersecret"}}"#).unwrap();
        let report = scan_path(&path, ScanFailOn::Warn).unwrap();
        assert!(report
            .findings
            .iter()
            .any(|f| f.kind == "unmarked_secret_intent"));
    }
}

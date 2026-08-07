//! HashiCorp Vault KV secret resolver (HTTP + token auth).
//!
//! Auth (outside the BCS file):
//! - `VAULT_ADDR` (required) — e.g. `https://vault.example.com`
//! - `VAULT_TOKEN` (required)
//! - `VAULT_NAMESPACE` (optional)
//! - `BCS_VAULT_TIMEOUT_SECS` (optional, default 10)
//!
//! Locator: `path#field` where `path` is the Vault API path after `/v1/`
//! (include `data` for KV v2), e.g. `secret/data/myapp#password`.

use crate::locator::{extract_secret_value, split_field};
use bcs_core::secret_resolver::SecretResolver;
use bcs_core::{BCSError, Result};
use std::time::Duration;

/// Resolves `vault:` secret references via the Vault HTTP API.
#[derive(Debug)]
pub struct VaultSecretResolver {
    addr: String,
    token: String,
    namespace: Option<String>,
    timeout: Duration,
}

impl VaultSecretResolver {
    /// Build a resolver from Vault environment variables (`VAULT_*`).
    pub fn from_env() -> Result<Self> {
        Self::from_env_keys(
            "vault",
            &["VAULT_ADDR"],
            &["VAULT_TOKEN"],
            &["VAULT_NAMESPACE"],
            "BCS_VAULT_TIMEOUT_SECS",
        )
    }

    /// Build a resolver from OpenBao environment variables (`BAO_*`, with Vault fallbacks).
    pub fn from_openbao_env() -> Result<Self> {
        Self::from_env_keys(
            "openbao",
            &["BAO_ADDR", "OPENBAO_ADDR", "VAULT_ADDR"],
            &["BAO_TOKEN", "OPENBAO_TOKEN", "VAULT_TOKEN"],
            &["BAO_NAMESPACE", "VAULT_NAMESPACE"],
            "BCS_OPENBAO_TIMEOUT_SECS",
        )
    }

    fn from_env_keys(
        label: &str,
        addr_keys: &[&str],
        token_keys: &[&str],
        namespace_keys: &[&str],
        timeout_key: &str,
    ) -> Result<Self> {
        let addr = first_env(addr_keys).ok_or_else(|| {
            BCSError::Decoding(format!(
                "{} is required when using the {} secret provider (tried: {})",
                addr_keys[0],
                label,
                addr_keys.join(", ")
            ))
        })?;
        let token = first_env(token_keys).ok_or_else(|| {
            BCSError::Decoding(format!(
                "{} is required when using the {} secret provider (tried: {})",
                token_keys[0],
                label,
                token_keys.join(", ")
            ))
        })?;
        if addr.trim().is_empty() || token.trim().is_empty() {
            return Err(BCSError::Decoding(format!(
                "{} address and token must be non-empty",
                label
            )));
        }

        let namespace = namespace_keys
            .iter()
            .find_map(|k| std::env::var(k).ok().filter(|s| !s.is_empty()));
        let timeout_secs = std::env::var(timeout_key)
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .or_else(|| {
                std::env::var("BCS_VAULT_TIMEOUT_SECS")
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok())
            })
            .unwrap_or(10);

        Ok(Self {
            addr: addr.trim_end_matches('/').to_string(),
            token,
            namespace,
            timeout: Duration::from_secs(timeout_secs.max(1)),
        })
    }

    /// Construct with explicit settings (useful for tests).
    pub fn new(
        addr: impl Into<String>,
        token: impl Into<String>,
        namespace: Option<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            addr: addr.into().trim_end_matches('/').to_string(),
            token: token.into(),
            namespace,
            timeout,
        }
    }

    fn fetch_json(&self, path: &str) -> Result<serde_json::Value> {
        let url = format!("{}/v1/{}", self.addr, path.trim_start_matches('/'));
        let mut request = crate::http_util::agent(self.timeout)
            .get(&url)
            .header("X-Vault-Token", &self.token);

        if let Some(ns) = &self.namespace {
            request = request.header("X-Vault-Namespace", ns);
        }

        let mut response = request
            .call()
            .map_err(|err| map_vault_http_error(path, err))?;

        let status = response.status().as_u16();
        let body = response.body_mut().read_to_string().map_err(|err| {
            BCSError::Decoding(format!(
                "Failed to read Vault/OpenBao response for '{}': {}",
                path, err
            ))
        })?;

        if !(200..300).contains(&status) {
            return Err(BCSError::Decoding(format!(
                "Vault/OpenBao request for '{}' failed with HTTP {}{}",
                path,
                status,
                sanitize_vault_error_body(&body)
            )));
        }

        serde_json::from_str(&body).map_err(|err| {
            BCSError::Decoding(format!(
                "Failed to parse Vault/OpenBao JSON for '{}': {}",
                path, err
            ))
        })
    }
}

fn first_env(keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| std::env::var(k).ok().filter(|s| !s.is_empty()))
}

impl SecretResolver for VaultSecretResolver {
    fn resolve(&self, scheme: &str, locator: &str) -> Result<String> {
        let (path, field) = split_field(locator);
        if path.is_empty() {
            return Err(BCSError::Decoding(format!(
                "{} secret locator must be non-empty (expected path or path#field)",
                scheme
            )));
        }

        let payload = self.fetch_json(path)?;
        let data = extract_vault_data(&payload).map_err(|msg| {
            BCSError::Decoding(format!(
                "Failed to resolve {}:'{}': {}",
                scheme, locator, msg
            ))
        })?;

        extract_secret_value(&data, field).map_err(|msg| {
            BCSError::Decoding(format!(
                "Failed to resolve {}:'{}': {}",
                scheme, locator, msg
            ))
        })
    }
}

/// Prefer KV v2 `data.data`, then KV v1 `data`, else the root object.
fn extract_vault_data(
    payload: &serde_json::Value,
) -> std::result::Result<serde_json::Value, String> {
    if let Some(data) = payload.get("data") {
        if let Some(inner) = data.get("data") {
            return Ok(inner.clone());
        }
        return Ok(data.clone());
    }
    Ok(payload.clone())
}

fn map_vault_http_error(path: &str, err: ureq::Error) -> BCSError {
    match err {
        ureq::Error::StatusCode(code) => BCSError::Decoding(format!(
            "Vault request for '{}' failed with HTTP {}",
            path, code
        )),
        other => BCSError::Decoding(format!(
            "Vault request for '{}' failed: {}",
            path,
            // Avoid leaking tokens that might appear in rare error strings.
            other.to_string().replace("VAULT_TOKEN", "[redacted]")
        )),
    }
}

fn sanitize_vault_error_body(body: &str) -> String {
    // Include errors[] messages when present; never echo raw secrets.
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(errors) = json.get("errors").and_then(|e| e.as_array()) {
            let joined: Vec<String> = errors
                .iter()
                .filter_map(|e| e.as_str().map(|s| s.to_string()))
                .collect();
            if !joined.is_empty() {
                return format!(": {}", joined.join("; "));
            }
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn extract_vault_data_kv2_and_kv1() {
        let kv2 = json!({"data": {"data": {"password": "x"}}});
        assert_eq!(extract_vault_data(&kv2).unwrap(), json!({"password": "x"}));

        let kv1 = json!({"data": {"password": "y"}});
        assert_eq!(extract_vault_data(&kv1).unwrap(), json!({"password": "y"}));
    }

    #[test]
    fn resolve_against_mock_http_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = crate::http_util::read_http_request(&mut stream);
            assert!(request.contains("GET /v1/secret/data/myapp"));
            assert!(request
                .to_ascii_lowercase()
                .contains("x-vault-token: test-token"));

            let body = r#"{"data":{"data":{"password":"from-vault"}}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let resolver = VaultSecretResolver::new(
            format!("http://{}", addr),
            "test-token",
            None,
            Duration::from_secs(2),
        );
        let value = resolver
            .resolve("vault", "secret/data/myapp#password")
            .unwrap();
        assert_eq!(value, "from-vault");
        server.join().unwrap();
    }

    #[test]
    fn missing_env_fails_clearly() {
        // Ensure we don't accidentally pick up a real token in CI.
        let had_addr = std::env::var_os("VAULT_ADDR");
        let had_token = std::env::var_os("VAULT_TOKEN");
        std::env::remove_var("VAULT_ADDR");
        std::env::remove_var("VAULT_TOKEN");
        let err = VaultSecretResolver::from_env().unwrap_err();
        assert!(err.to_string().contains("VAULT_ADDR"));
        if let Some(v) = had_addr {
            std::env::set_var("VAULT_ADDR", v);
        }
        if let Some(v) = had_token {
            std::env::set_var("VAULT_TOKEN", v);
        }
    }
}

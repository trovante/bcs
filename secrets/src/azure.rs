//! Azure Key Vault secret resolver (HTTP + bearer / client credentials).
//!
//! Auth (outside the BCS file):
//! - `AZURE_ACCESS_TOKEN` — bearer token (optional if client credentials are set)
//! - or `AZURE_TENANT_ID` + `AZURE_CLIENT_ID` + `AZURE_CLIENT_SECRET` (client credentials)
//! - `AZURE_KEY_VAULT_URL` — optional default vault base when locator is only a secret name
//! - `BCS_AZURE_TIMEOUT_SECS` (optional, default 10)
//!
//! Locator forms (optional `#json_field`):
//! - Full URI: `https://myvault.vault.azure.net/secrets/db-password[/version]`
//! - `vaultName/secretName[/version]`
//! - `secretName` (requires `AZURE_KEY_VAULT_URL`)

use crate::http_util::{finalize_optional_json_field, map_http_error};
use crate::locator::split_field;
use bcs_core::secret_resolver::SecretResolver;
use bcs_core::{BCSError, Result};
use std::time::Duration;

const API_VERSION: &str = "7.4";

/// Resolves `azure:` secret references via Azure Key Vault.
#[derive(Debug)]
pub struct AzureSecretResolver {
    access_token: String,
    default_vault_url: Option<String>,
    timeout: Duration,
    /// Override host for tests: rewrites `https://host/path` → `{prefix}/path`.
    endpoint_override_prefix: Option<String>,
}

impl AzureSecretResolver {
    /// Build from environment variables.
    pub fn from_env() -> Result<Self> {
        let timeout_secs = std::env::var("BCS_AZURE_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(10);
        let timeout = Duration::from_secs(timeout_secs.max(1));
        let access_token = resolve_azure_token(None, timeout)?;
        let default_vault_url = std::env::var("AZURE_KEY_VAULT_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .map(|s| s.trim_end_matches('/').to_string());

        Ok(Self {
            access_token,
            default_vault_url,
            timeout,
            endpoint_override_prefix: None,
        })
    }

    /// Test helper with an explicit bearer token and optional URL rewrite prefix.
    pub fn new_for_test(
        access_token: impl Into<String>,
        default_vault_url: Option<String>,
        endpoint_override_prefix: Option<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            access_token: access_token.into(),
            default_vault_url: default_vault_url.map(|s| s.trim_end_matches('/').to_string()),
            timeout,
            endpoint_override_prefix,
        }
    }

    fn build_secret_url(&self, resource: &str) -> Result<String> {
        let base = if resource.starts_with("https://") || resource.starts_with("http://") {
            resource.trim_end_matches('/').to_string()
        } else if let Some((vault, rest)) = resource.split_once('/') {
            if rest.is_empty() {
                return Err(BCSError::Decoding(
                    "Azure locator 'vault/secret' requires a secret name".to_string(),
                ));
            }
            format!(
                "https://{}.vault.azure.net/secrets/{}",
                vault,
                rest.trim_start_matches('/')
            )
        } else {
            let vault = self.default_vault_url.as_deref().ok_or_else(|| {
                BCSError::Decoding(
                    "Azure locator is a bare secret name; set AZURE_KEY_VAULT_URL or use vault/secret or a full URI".to_string(),
                )
            })?;
            format!("{}/secrets/{}", vault.trim_end_matches('/'), resource)
        };

        let mut url = if base.contains('?') {
            format!("{}&api-version={}", base, API_VERSION)
        } else {
            format!("{}?api-version={}", base, API_VERSION)
        };

        if let Some(prefix) = &self.endpoint_override_prefix {
            if let Some(idx) = url.find("://") {
                let after_scheme = &url[idx + 3..];
                if let Some(path_idx) = after_scheme.find('/') {
                    url = format!(
                        "{}{}",
                        prefix.trim_end_matches('/'),
                        &after_scheme[path_idx..]
                    );
                } else {
                    url = prefix.trim_end_matches('/').to_string();
                }
            }
        }

        Ok(url)
    }

    fn fetch_secret_string(&self, resource: &str) -> Result<String> {
        let url = self.build_secret_url(resource)?;
        let response = ureq::get(&url)
            .timeout(self.timeout)
            .set("Authorization", &format!("Bearer {}", self.access_token))
            .call()
            .map_err(|err| map_http_error("Azure Key Vault", resource, err))?;

        let status = response.status();
        let body = response.into_string().map_err(|err| {
            BCSError::Decoding(format!(
                "Failed to read Azure Key Vault response for '{}': {}",
                resource, err
            ))
        })?;

        if !(200..300).contains(&status) {
            return Err(crate::http_util::classify_status(
                "Azure Key Vault",
                resource,
                status,
                &body,
            ));
        }

        let json: serde_json::Value = serde_json::from_str(&body).map_err(|err| {
            BCSError::Decoding(format!(
                "Failed to parse Azure Key Vault JSON for '{}': {}",
                resource, err
            ))
        })?;

        json.get("value")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                BCSError::Decoding(format!(
                    "Azure Key Vault secret '{}' returned no string value",
                    resource
                ))
            })
    }
}

impl SecretResolver for AzureSecretResolver {
    fn resolve(&self, _scheme: &str, locator: &str) -> Result<String> {
        let (resource, field) = split_field(locator);
        if resource.is_empty() {
            return Err(BCSError::Decoding(
                "Azure secret locator must be non-empty".to_string(),
            ));
        }

        let secret_string = self.fetch_secret_string(resource)?;
        finalize_optional_json_field("azure", locator, resource, &secret_string, field)
    }
}

pub(crate) fn resolve_azure_token(
    token_endpoint_override: Option<&str>,
    timeout: Duration,
) -> Result<String> {
    if let Ok(token) = std::env::var("AZURE_ACCESS_TOKEN") {
        if !token.trim().is_empty() {
            return Ok(token);
        }
    }

    let tenant = std::env::var("AZURE_TENANT_ID").map_err(|_| missing_azure_auth())?;
    let client_id = std::env::var("AZURE_CLIENT_ID").map_err(|_| missing_azure_auth())?;
    let client_secret = std::env::var("AZURE_CLIENT_SECRET").map_err(|_| missing_azure_auth())?;
    if tenant.trim().is_empty() || client_id.trim().is_empty() || client_secret.trim().is_empty() {
        return Err(missing_azure_auth());
    }

    let token_url = token_endpoint_override
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            format!(
                "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
                tenant
            )
        });

    let form = format!(
        "grant_type=client_credentials&client_id={}&client_secret={}&scope={}",
        urlencoding_lite(&client_id),
        urlencoding_lite(&client_secret),
        urlencoding_lite("https://vault.azure.net/.default")
    );

    let response = ureq::post(&token_url)
        .timeout(timeout)
        .set("content-type", "application/x-www-form-urlencoded")
        .send_string(&form)
        .map_err(|_| {
            BCSError::Decoding(
                "Azure client-credentials token request failed (unavailable)".to_string(),
            )
        })?;

    let status = response.status();
    let body = response.into_string().map_err(|err| {
        BCSError::Decoding(format!("Failed to read Azure token response: {}", err))
    })?;
    if !(200..300).contains(&status) {
        return Err(BCSError::Decoding(
            "Azure client-credentials token request failed (denied)".to_string(),
        ));
    }

    let json: serde_json::Value = serde_json::from_str(&body)
        .map_err(|err| BCSError::Decoding(format!("Failed to parse Azure token JSON: {}", err)))?;
    json.get("access_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| BCSError::Decoding("Azure token response missing access_token".to_string()))
}

fn missing_azure_auth() -> BCSError {
    BCSError::Decoding(
        "Azure auth required: set AZURE_ACCESS_TOKEN, or AZURE_TENANT_ID + AZURE_CLIENT_ID + AZURE_CLIENT_SECRET"
            .to_string(),
    )
}

fn urlencoding_lite(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn from_env_requires_auth() {
        let keys = [
            "AZURE_ACCESS_TOKEN",
            "AZURE_TENANT_ID",
            "AZURE_CLIENT_ID",
            "AZURE_CLIENT_SECRET",
        ];
        let saved: Vec<_> = keys.iter().map(|k| (*k, std::env::var_os(k))).collect();
        for k in &keys {
            std::env::remove_var(k);
        }
        let err = AzureSecretResolver::from_env().unwrap_err();
        assert!(err.to_string().contains("Azure auth required"));
        for (k, v) in saved {
            if let Some(val) = v {
                std::env::set_var(k, val);
            }
        }
    }

    #[test]
    fn resolve_against_mock_http_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 8192];
            let n = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]);
            assert!(request.contains("GET /secrets/db-password"));
            assert!(request
                .to_ascii_lowercase()
                .contains("authorization: bearer"));
            assert!(request.contains("api-version=7.4"));

            let body = r#"{"value":"{\"password\":\"from-azure\"}"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let resolver = AzureSecretResolver::new_for_test(
            "test-token",
            Some(format!("http://{}", addr)),
            None,
            Duration::from_secs(2),
        );
        let value = resolver.resolve("azure", "db-password#password").unwrap();
        assert_eq!(value, "from-azure");
        server.join().unwrap();
    }

    #[test]
    fn build_url_from_vault_slash_secret() {
        let resolver = AzureSecretResolver::new_for_test("t", None, None, Duration::from_secs(1));
        let url = resolver.build_secret_url("myvault/db-pass").unwrap();
        assert!(url.starts_with("https://myvault.vault.azure.net/secrets/db-pass?"));
    }
}

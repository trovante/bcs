//! HashiCorp Vault / OpenBao Transit encrypt/decrypt KeyWrapper.
//!
//! Auth: `VAULT_ADDR` + `VAULT_TOKEN` (or `BAO_*` for openbao).
//! KEK locator: transit key name (e.g. `app-dek`) or `transit/keys/app-dek` style path;
//! the wrapper posts to `/v1/transit/encrypt/{key}` and `/v1/transit/decrypt/{key}`.

use bcs_core::security::KeyWrapper;
use bcs_core::{BCSError, Result};
use std::time::Duration;

const DEK_LEN: usize = 32;

/// Wraps/unwraps DEKs via Vault Transit secrets engine.
#[derive(Debug)]
pub struct VaultTransitKeyWrapper {
    addr: String,
    token: String,
    namespace: Option<String>,
    timeout: Duration,
    /// Stored provider label for ensure_provider (`vault` or `openbao`).
    provider_label: String,
}

impl VaultTransitKeyWrapper {
    pub fn from_env() -> Result<Self> {
        Self::from_env_keys(
            "vault",
            &["VAULT_ADDR"],
            &["VAULT_TOKEN"],
            &["VAULT_NAMESPACE"],
            "BCS_VAULT_TIMEOUT_SECS",
        )
    }

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
                "{} is required for {} transit wrap (tried: {})",
                addr_keys[0],
                label,
                addr_keys.join(", ")
            ))
        })?;
        let token = first_env(token_keys).ok_or_else(|| {
            BCSError::Decoding(format!(
                "{} is required for {} transit wrap (tried: {})",
                token_keys[0],
                label,
                token_keys.join(", ")
            ))
        })?;
        if addr.trim().is_empty() || token.trim().is_empty() {
            return Err(BCSError::Decoding(format!(
                "{} address and token must be non-empty for transit",
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
            provider_label: label.to_string(),
        })
    }

    fn transit_key_name(locator: &str) -> &str {
        locator
            .trim_start_matches('/')
            .trim_start_matches("transit/keys/")
            .trim_start_matches("transit/encrypt/")
            .trim_start_matches("transit/decrypt/")
            .split('/')
            .next()
            .unwrap_or(locator)
    }

    fn post_json(&self, path: &str, body: &str) -> Result<serde_json::Value> {
        let url = format!("{}/v1/{}", self.addr, path.trim_start_matches('/'));
        let mut request = ureq::post(&url)
            .timeout(self.timeout)
            .set("X-Vault-Token", &self.token)
            .set("Content-Type", "application/json");
        if let Some(ns) = &self.namespace {
            request = request.set("X-Vault-Namespace", ns);
        }
        let response = request.send_string(body).map_err(|err| {
            BCSError::Decoding(format!(
                "{} transit request failed: {}",
                self.provider_label,
                status_only(err)
            ))
        })?;
        let status = response.status();
        let resp = response.into_string().map_err(|err| {
            BCSError::Decoding(format!("Failed to read transit response: {}", err))
        })?;
        if !(200..300).contains(&status) {
            return Err(BCSError::Decoding(format!(
                "{} transit HTTP {}",
                self.provider_label, status
            )));
        }
        serde_json::from_str(&resp)
            .map_err(|err| BCSError::Decoding(format!("Invalid transit JSON: {}", err)))
    }
}

impl KeyWrapper for VaultTransitKeyWrapper {
    fn wrap(&self, provider: &str, kek_locator: &str, dek: &[u8]) -> Result<Vec<u8>> {
        self.ensure_provider(provider)?;
        if dek.len() != DEK_LEN {
            return Err(BCSError::Encoding(
                "Vault transit wrap expects a 32-byte DEK".to_string(),
            ));
        }
        let key = Self::transit_key_name(kek_locator);
        let plaintext = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, dek);
        let body = serde_json::json!({ "plaintext": plaintext }).to_string();
        let json = self.post_json(&format!("transit/encrypt/{}", key), &body)?;
        let ciphertext = json
            .pointer("/data/ciphertext")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                BCSError::Encoding("Vault transit encrypt missing data.ciphertext".to_string())
            })?;
        // Store Vault ciphertext string bytes (e.g. vault:v1:...) as wrapped DEK blob.
        Ok(ciphertext.as_bytes().to_vec())
    }

    fn unwrap(
        &self,
        provider: &str,
        kek_locator: &str,
        wrapped_dek: &[u8],
    ) -> Result<[u8; DEK_LEN]> {
        self.ensure_provider(provider)?;
        let key = Self::transit_key_name(kek_locator);
        let ciphertext = std::str::from_utf8(wrapped_dek).map_err(|_| {
            BCSError::Decoding(
                "Vault transit wrapped DEK is not valid UTF-8 ciphertext".to_string(),
            )
        })?;
        let body = serde_json::json!({ "ciphertext": ciphertext }).to_string();
        let json = self.post_json(&format!("transit/decrypt/{}", key), &body)?;
        let plaintext_b64 = json
            .pointer("/data/plaintext")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                BCSError::Decoding("Vault transit decrypt missing data.plaintext".to_string())
            })?;
        let bytes =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, plaintext_b64)
                .map_err(|_| {
                    BCSError::Decoding("Vault transit plaintext was not valid base64".into())
                })?;
        if bytes.len() != DEK_LEN {
            return Err(BCSError::Decoding(format!(
                "Vault transit decrypt returned {} bytes, expected {}",
                bytes.len(),
                DEK_LEN
            )));
        }
        let mut out = [0u8; DEK_LEN];
        out.copy_from_slice(&bytes);
        Ok(out)
    }
}

impl VaultTransitKeyWrapper {
    fn ensure_provider(&self, provider: &str) -> Result<()> {
        let ok = match self.provider_label.as_str() {
            "vault" => matches!(provider, "vault" | "vault-transit" | "transit"),
            "openbao" => matches!(provider, "openbao" | "bao" | "vault-transit" | "transit"),
            _ => false,
        };
        if ok {
            Ok(())
        } else {
            Err(BCSError::Decoding(format!(
                "VaultTransitKeyWrapper ({}) does not handle provider '{}'",
                self.provider_label, provider
            )))
        }
    }
}

fn first_env(keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| std::env::var(k).ok().filter(|s| !s.is_empty()))
}

fn status_only(err: ureq::Error) -> String {
    match err {
        ureq::Error::Status(code, _) => format!("HTTP {}", code),
        _ => "unavailable".to_string(),
    }
}

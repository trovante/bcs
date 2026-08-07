//! Azure Key Vault wrapkey/unwrapkey KeyWrapper.
//!
//! Auth: same as Azure secret resolver (`AZURE_ACCESS_TOKEN` or client credentials).
//! KEK locator forms:
//! - Full key URI: `https://myvault.vault.azure.net/keys/my-key[/version]`
//! - `vaultName/keyName[/version]`
//! - `keyName` (requires `AZURE_KEY_VAULT_URL`)

use crate::azure::resolve_azure_token;
use bcs_core::security::KeyWrapper;
use bcs_core::{BCSError, Result};
use std::time::Duration;

const API_VERSION: &str = "7.4";
const DEK_LEN: usize = 32;
const WRAP_ALG: &str = "RSA-OAEP-256";

/// Wraps/unwraps DEKs via Azure Key Vault cryptographic wrap operations.
#[derive(Debug)]
pub struct AzureKmsKeyWrapper {
    access_token: String,
    default_vault_url: Option<String>,
    timeout: Duration,
}

impl AzureKmsKeyWrapper {
    pub fn from_env() -> Result<Self> {
        let timeout_secs = std::env::var("BCS_AZURE_KMS_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .or_else(|| {
                std::env::var("BCS_AZURE_TIMEOUT_SECS")
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok())
            })
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
        })
    }

    fn key_url(&self, locator: &str, op: &str) -> Result<String> {
        let base = if locator.starts_with("https://") || locator.starts_with("http://") {
            locator.trim_end_matches('/').to_string()
        } else if let Some((vault, rest)) = locator.split_once('/') {
            if rest.is_empty() {
                return Err(BCSError::Decoding(
                    "Azure KMS locator 'vault/key' requires a key name".to_string(),
                ));
            }
            format!("https://{}.vault.azure.net/keys/{}", vault, rest)
        } else {
            let vault = self.default_vault_url.as_deref().ok_or_else(|| {
                BCSError::Decoding(
                    "Azure KMS locator is a bare key name; set AZURE_KEY_VAULT_URL or use vault/key"
                        .to_string(),
                )
            })?;
            // AZURE_KEY_VAULT_URL is vault base (no /keys)
            format!("{}/keys/{}", vault.trim_end_matches('/'), locator)
        };
        Ok(format!("{}/{}?api-version={}", base, op, API_VERSION))
    }

    fn post_json(&self, url: &str, body: &str) -> Result<serde_json::Value> {
        let mut response = crate::http_util::agent(self.timeout)
            .post(url)
            .header("Authorization", &format!("Bearer {}", self.access_token))
            .header("Content-Type", "application/json")
            .send(body)
            .map_err(|err| {
                BCSError::Decoding(format!(
                    "Azure Key Vault KMS request failed: {}",
                    crate::http_util::status_only(err)
                ))
            })?;
        let status = response.status().as_u16();
        let resp = response.body_mut().read_to_string().map_err(|err| {
            BCSError::Decoding(format!("Failed to read Azure KMS response: {}", err))
        })?;
        if !(200..300).contains(&status) {
            return Err(BCSError::Decoding(format!(
                "Azure Key Vault KMS HTTP {}",
                status
            )));
        }
        serde_json::from_str(&resp)
            .map_err(|err| BCSError::Decoding(format!("Invalid Azure KMS JSON: {}", err)))
    }
}

impl KeyWrapper for AzureKmsKeyWrapper {
    fn wrap(&self, provider: &str, kek_locator: &str, dek: &[u8]) -> Result<Vec<u8>> {
        ensure_provider(provider)?;
        if dek.len() != DEK_LEN {
            return Err(BCSError::Encoding(
                "Azure KMS wrap expects a 32-byte DEK".to_string(),
            ));
        }
        let url = self.key_url(kek_locator, "wrapkey")?;
        let value = base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, dek);
        let body = serde_json::json!({ "alg": WRAP_ALG, "value": value }).to_string();
        let json = self.post_json(&url, &body)?;
        let wrapped = json
            .get("value")
            .and_then(|v| v.as_str())
            .ok_or_else(|| BCSError::Encoding("Azure wrapkey missing value".to_string()))?;
        decode_azure_b64(wrapped)
            .map_err(|_| BCSError::Encoding("Azure wrapkey value was not valid base64".into()))
    }

    fn unwrap(
        &self,
        provider: &str,
        kek_locator: &str,
        wrapped_dek: &[u8],
    ) -> Result<[u8; DEK_LEN]> {
        ensure_provider(provider)?;
        let url = self.key_url(kek_locator, "unwrapkey")?;
        let value = base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            wrapped_dek,
        );
        let body = serde_json::json!({ "alg": WRAP_ALG, "value": value }).to_string();
        let json = self.post_json(&url, &body)?;
        let plain = json
            .get("value")
            .and_then(|v| v.as_str())
            .ok_or_else(|| BCSError::Decoding("Azure unwrapkey missing value".to_string()))?;
        let bytes = decode_azure_b64(plain)
            .map_err(|_| BCSError::Decoding("Azure unwrapkey value was not valid base64".into()))?;
        if bytes.len() != DEK_LEN {
            return Err(BCSError::Decoding(format!(
                "Azure unwrapkey returned {} bytes, expected {}",
                bytes.len(),
                DEK_LEN
            )));
        }
        let mut out = [0u8; DEK_LEN];
        out.copy_from_slice(&bytes);
        Ok(out)
    }
}

fn ensure_provider(provider: &str) -> Result<()> {
    if matches!(provider, "azure" | "azure-kms" | "akv") {
        Ok(())
    } else {
        Err(BCSError::Decoding(format!(
            "AzureKmsKeyWrapper does not handle provider '{}'",
            provider
        )))
    }
}

fn decode_azure_b64(s: &str) -> std::result::Result<Vec<u8>, ()> {
    base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, s)
        .or_else(|_| base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE, s))
        .or_else(|_| base64::Engine::decode(&base64::engine::general_purpose::STANDARD, s))
        .map_err(|_| ())
}

//! Google Cloud KMS Encrypt/Decrypt KeyWrapper.
//!
//! Auth: `GOOGLE_ACCESS_TOKEN` / `GCP_ACCESS_TOKEN`.
//! KEK locator: full CryptoKey resource name, e.g.
//! `projects/P/locations/L/keyRings/R/cryptoKeys/K` (encrypt uses `/cryptoKeyVersions/...`
//! when a version is appended; otherwise uses the key's primary encrypt path).

use bcs_core::security::KeyWrapper;
use bcs_core::{BCSError, Result};
use std::time::Duration;

const DEK_LEN: usize = 32;

/// Wraps/unwraps DEKs via Google Cloud KMS.
#[derive(Debug)]
pub struct GcpKmsKeyWrapper {
    access_token: String,
    timeout: Duration,
    api_base: String,
}

impl GcpKmsKeyWrapper {
    pub fn from_env() -> Result<Self> {
        let access_token = std::env::var("GOOGLE_ACCESS_TOKEN")
            .or_else(|_| std::env::var("GCP_ACCESS_TOKEN"))
            .map_err(|_| {
                BCSError::Decoding(
                    "GOOGLE_ACCESS_TOKEN or GCP_ACCESS_TOKEN is required for GCP KMS wrap"
                        .to_string(),
                )
            })?;
        if access_token.trim().is_empty() {
            return Err(BCSError::Decoding(
                "GOOGLE_ACCESS_TOKEN / GCP_ACCESS_TOKEN must be non-empty".to_string(),
            ));
        }
        let timeout_secs = std::env::var("BCS_GCP_KMS_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .or_else(|| {
                std::env::var("BCS_GCP_TIMEOUT_SECS")
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok())
            })
            .unwrap_or(10);
        Ok(Self {
            access_token,
            timeout: Duration::from_secs(timeout_secs.max(1)),
            api_base: "https://cloudkms.googleapis.com".to_string(),
        })
    }

    pub fn new_for_test(
        access_token: impl Into<String>,
        api_base: impl Into<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            access_token: access_token.into(),
            timeout,
            api_base: api_base.into().trim_end_matches('/').to_string(),
        }
    }

    fn post_json(&self, url: &str, body: &str) -> Result<serde_json::Value> {
        let mut response = crate::http_util::agent(self.timeout)
            .post(url)
            .header("Authorization", &format!("Bearer {}", self.access_token))
            .header("Content-Type", "application/json")
            .send(body)
            .map_err(|err| {
                BCSError::Decoding(format!(
                    "GCP KMS request failed: {}",
                    crate::http_util::status_only(err)
                ))
            })?;
        let status = response.status().as_u16();
        let resp = response.body_mut().read_to_string().map_err(|err| {
            BCSError::Decoding(format!("Failed to read GCP KMS response: {}", err))
        })?;
        if !(200..300).contains(&status) {
            return Err(BCSError::Decoding(format!("GCP KMS HTTP {}", status)));
        }
        serde_json::from_str(&resp)
            .map_err(|err| BCSError::Decoding(format!("Invalid GCP KMS JSON: {}", err)))
    }
}

impl KeyWrapper for GcpKmsKeyWrapper {
    fn wrap(&self, provider: &str, kek_locator: &str, dek: &[u8]) -> Result<Vec<u8>> {
        ensure_provider(provider)?;
        if dek.len() != DEK_LEN {
            return Err(BCSError::Encoding(
                "GCP KMS wrap expects a 32-byte DEK".to_string(),
            ));
        }
        let name = kek_locator.trim_start_matches('/');
        let url = format!("{}/v1/{}:encrypt", self.api_base, name);
        let plaintext = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, dek);
        let body = serde_json::json!({ "plaintext": plaintext }).to_string();
        let json = self.post_json(&url, &body)?;
        let ciphertext = json
            .get("ciphertext")
            .and_then(|v| v.as_str())
            .ok_or_else(|| BCSError::Encoding("GCP KMS encrypt missing ciphertext".to_string()))?;
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, ciphertext)
            .map_err(|_| BCSError::Encoding("GCP KMS ciphertext was not valid base64".into()))
    }

    fn unwrap(
        &self,
        provider: &str,
        kek_locator: &str,
        wrapped_dek: &[u8],
    ) -> Result<[u8; DEK_LEN]> {
        ensure_provider(provider)?;
        let name = kek_locator.trim_start_matches('/');
        // Decrypt is typically on cryptoKeys/... or cryptoKeyVersions/...
        let url = format!("{}/v1/{}:decrypt", self.api_base, name);
        let ciphertext =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, wrapped_dek);
        let body = serde_json::json!({ "ciphertext": ciphertext }).to_string();
        let json = self.post_json(&url, &body)?;
        let plaintext_b64 = json
            .get("plaintext")
            .and_then(|v| v.as_str())
            .ok_or_else(|| BCSError::Decoding("GCP KMS decrypt missing plaintext".to_string()))?;
        let bytes =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, plaintext_b64)
                .map_err(|_| BCSError::Decoding("GCP KMS plaintext was not valid base64".into()))?;
        if bytes.len() != DEK_LEN {
            return Err(BCSError::Decoding(format!(
                "GCP KMS decrypt returned {} bytes, expected {}",
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
    if matches!(provider, "gcp" | "gcp-kms" | "google") {
        Ok(())
    } else {
        Err(BCSError::Decoding(format!(
            "GcpKmsKeyWrapper does not handle provider '{}'",
            provider
        )))
    }
}

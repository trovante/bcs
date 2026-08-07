//! AWS KMS Encrypt/Decrypt KeyWrapper (SigV4 + ureq).
//!
//! Auth (same as Secrets Manager):
//! - `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, optional `AWS_SESSION_TOKEN`
//! - `AWS_REGION` / `AWS_DEFAULT_REGION`
//! - `BCS_AWS_KMS_TIMEOUT_SECS` (optional, default 10)
//!
//! KEK locator: KMS key id, ARN, or alias (`alias/my-key`).

use bcs_core::security::KeyWrapper;
use bcs_core::{BCSError, Result};
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

const DEK_LEN: usize = 32;

/// Wraps/unwraps DEKs via AWS KMS Encrypt / Decrypt.
#[derive(Debug)]
pub struct AwsKmsKeyWrapper {
    access_key: String,
    secret_key: String,
    session_token: Option<String>,
    region: String,
    timeout: Duration,
    endpoint_override: Option<String>,
}

impl AwsKmsKeyWrapper {
    pub fn from_env() -> Result<Self> {
        let access_key = std::env::var("AWS_ACCESS_KEY_ID").map_err(|_| {
            BCSError::Decoding("AWS_ACCESS_KEY_ID is required for AWS KMS wrap".to_string())
        })?;
        let secret_key = std::env::var("AWS_SECRET_ACCESS_KEY").map_err(|_| {
            BCSError::Decoding("AWS_SECRET_ACCESS_KEY is required for AWS KMS wrap".to_string())
        })?;
        let region = std::env::var("AWS_REGION")
            .or_else(|_| std::env::var("AWS_DEFAULT_REGION"))
            .map_err(|_| {
                BCSError::Decoding(
                    "AWS_REGION or AWS_DEFAULT_REGION is required for AWS KMS wrap".to_string(),
                )
            })?;
        if access_key.trim().is_empty() || secret_key.trim().is_empty() || region.trim().is_empty()
        {
            return Err(BCSError::Decoding(
                "AWS credentials and region must be non-empty for KMS".to_string(),
            ));
        }
        let session_token = std::env::var("AWS_SESSION_TOKEN")
            .ok()
            .filter(|s| !s.is_empty());
        let timeout_secs = std::env::var("BCS_AWS_KMS_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .or_else(|| {
                std::env::var("BCS_AWS_SECRET_TIMEOUT_SECS")
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok())
            })
            .unwrap_or(10);

        Ok(Self {
            access_key,
            secret_key,
            session_token,
            region,
            timeout: Duration::from_secs(timeout_secs.max(1)),
            endpoint_override: None,
        })
    }

    pub fn new_for_test(
        access_key: impl Into<String>,
        secret_key: impl Into<String>,
        region: impl Into<String>,
        endpoint_override: impl Into<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            access_key: access_key.into(),
            secret_key: secret_key.into(),
            session_token: None,
            region: region.into(),
            timeout,
            endpoint_override: Some(endpoint_override.into()),
        }
    }

    fn endpoint(&self) -> String {
        if let Some(endpoint) = &self.endpoint_override {
            return endpoint.trim_end_matches('/').to_string();
        }
        format!("https://kms.{}.amazonaws.com", self.region)
    }

    fn call_kms(&self, target: &str, body: &str) -> Result<serde_json::Value> {
        let endpoint = self.endpoint();
        let host = host_from_endpoint(&endpoint)?;
        let url = format!("{}/", endpoint.trim_end_matches('/'));
        let body_hash = hex::encode(Sha256::digest(body.as_bytes()));

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| BCSError::Decoding(format!("system clock error: {}", err)))?;
        let datetime = format_amz_datetime(now.as_secs());
        let date = &datetime[..8];

        let mut canonical_headers = format!(
            "content-type:application/x-amz-json-1.1\nhost:{}\nx-amz-date:{}\nx-amz-target:{}\n",
            host, datetime, target
        );
        let mut signed_headers = "content-type;host;x-amz-date;x-amz-target".to_string();
        if let Some(token) = &self.session_token {
            canonical_headers.push_str(&format!("x-amz-security-token:{}\n", token));
            signed_headers.push_str(";x-amz-security-token");
        }

        let canonical_request =
            format!("POST\n/\n\n{canonical_headers}\n{signed_headers}\n{body_hash}");
        let canonical_hash = hex::encode(Sha256::digest(canonical_request.as_bytes()));
        let credential_scope = format!("{}/{}/kms/aws4_request", date, self.region);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{}\n{}\n{}",
            datetime, credential_scope, canonical_hash
        );
        let signing_key = aws4_signing_key(&self.secret_key, date, &self.region, "kms");
        let signature = hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes()));
        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
            self.access_key, credential_scope, signed_headers, signature
        );

        let mut request = crate::http_util::agent(self.timeout)
            .post(&url)
            .header("content-type", "application/x-amz-json-1.1")
            .header("x-amz-target", target)
            .header("x-amz-date", &datetime)
            .header("authorization", &authorization);
        if let Some(token) = &self.session_token {
            request = request.header("x-amz-security-token", token);
        }

        let mut response = request.send(body).map_err(|err| {
            BCSError::Decoding(format!("AWS KMS request failed: {}", redact_ureq(err)))
        })?;
        let status = response.status().as_u16();
        let resp_body = response.body_mut().read_to_string().map_err(|err| {
            BCSError::Decoding(format!("Failed to read AWS KMS response: {}", err))
        })?;
        if !(200..300).contains(&status) {
            return Err(BCSError::Decoding(format!(
                "AWS KMS HTTP {} (details omitted)",
                status
            )));
        }
        serde_json::from_str(&resp_body)
            .map_err(|err| BCSError::Decoding(format!("Invalid AWS KMS JSON: {}", err)))
    }
}

impl KeyWrapper for AwsKmsKeyWrapper {
    fn wrap(&self, provider: &str, kek_locator: &str, dek: &[u8]) -> Result<Vec<u8>> {
        ensure_provider(provider)?;
        if dek.len() != DEK_LEN {
            return Err(BCSError::Encoding(
                "AWS KMS wrap expects a 32-byte DEK".to_string(),
            ));
        }
        let plaintext = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, dek);
        let body = serde_json::json!({
            "KeyId": kek_locator,
            "Plaintext": plaintext,
        })
        .to_string();
        let json = self.call_kms("TrentService.Encrypt", &body)?;
        let blob = json
            .get("CiphertextBlob")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                BCSError::Encoding("AWS KMS Encrypt response missing CiphertextBlob".to_string())
            })?;
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, blob)
            .map_err(|_| BCSError::Encoding("AWS KMS CiphertextBlob was not valid base64".into()))
    }

    fn unwrap(
        &self,
        provider: &str,
        _kek_locator: &str,
        wrapped_dek: &[u8],
    ) -> Result<[u8; DEK_LEN]> {
        ensure_provider(provider)?;
        let blob = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, wrapped_dek);
        let body = serde_json::json!({ "CiphertextBlob": blob }).to_string();
        let json = self.call_kms("TrentService.Decrypt", &body)?;
        let plaintext_b64 = json
            .get("Plaintext")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                BCSError::Decoding("AWS KMS Decrypt response missing Plaintext".to_string())
            })?;
        let bytes =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, plaintext_b64)
                .map_err(|_| BCSError::Decoding("AWS KMS Plaintext was not valid base64".into()))?;
        if bytes.len() != DEK_LEN {
            return Err(BCSError::Decoding(format!(
                "AWS KMS Decrypt returned {} bytes, expected {}",
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
    if provider == "aws" || provider == "aws-kms" {
        Ok(())
    } else {
        Err(BCSError::Decoding(format!(
            "AwsKmsKeyWrapper does not handle provider '{}'",
            provider
        )))
    }
}

fn host_from_endpoint(endpoint: &str) -> Result<String> {
    let without_scheme = endpoint
        .strip_prefix("https://")
        .or_else(|| endpoint.strip_prefix("http://"))
        .unwrap_or(endpoint);
    Ok(without_scheme.trim_end_matches('/').to_string())
}

fn format_amz_datetime(epoch_secs: u64) -> String {
    let days = epoch_secs / 86400;
    let tod = epoch_secs % 86400;
    let (year, month, day) = civil_from_days(days as i64);
    let hour = tod / 3600;
    let min = (tod % 3600) / 60;
    let sec = tod % 60;
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        year, month, day, hour, min, sec
    )
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn aws4_signing_key(secret: &str, date: &str, region: &str, service: &str) -> Vec<u8> {
    let k_date = hmac_sha256(format!("AWS4{}", secret).as_bytes(), date.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    hmac_sha256(&k_service, b"aws4_request")
}

fn redact_ureq(err: ureq::Error) -> String {
    match err {
        ureq::Error::StatusCode(code) => format!("HTTP {}", code),
        _ => "unavailable".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_env_requires_credentials() {
        let had = std::env::var_os("AWS_ACCESS_KEY_ID");
        std::env::remove_var("AWS_ACCESS_KEY_ID");
        let err = AwsKmsKeyWrapper::from_env().unwrap_err();
        assert!(err.to_string().contains("AWS_ACCESS_KEY_ID"));
        if let Some(v) = had {
            std::env::set_var("AWS_ACCESS_KEY_ID", v);
        }
    }
}

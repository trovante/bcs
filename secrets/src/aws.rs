//! AWS Secrets Manager resolver (minimal SigV4 + ureq).
//!
//! Auth (outside the BCS file) — environment credentials for phase 3a:
//! - `AWS_ACCESS_KEY_ID` (required)
//! - `AWS_SECRET_ACCESS_KEY` (required)
//! - `AWS_SESSION_TOKEN` (optional)
//! - `AWS_REGION` or `AWS_DEFAULT_REGION` (required)
//! - `BCS_AWS_SECRET_TIMEOUT_SECS` (optional, default 10)
//!
//! Locator: `secret-id` or `secret-id#json_field` (name or ARN).

use crate::locator::{extract_secret_value, split_field};
use bcs_core::secret_resolver::SecretResolver;
use bcs_core::{BCSError, Result};
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

/// Resolves `aws:` secret references via AWS Secrets Manager.
#[derive(Debug)]
pub struct AwsSecretResolver {
    access_key: String,
    secret_key: String,
    session_token: Option<String>,
    region: String,
    timeout: Duration,
    /// Override endpoint for tests (`http://127.0.0.1:port`).
    endpoint_override: Option<String>,
}

impl AwsSecretResolver {
    /// Build from environment credential variables.
    pub fn from_env() -> Result<Self> {
        let access_key = std::env::var("AWS_ACCESS_KEY_ID").map_err(|_| {
            BCSError::Decoding(
                "AWS_ACCESS_KEY_ID is required when using the aws secret provider".to_string(),
            )
        })?;
        let secret_key = std::env::var("AWS_SECRET_ACCESS_KEY").map_err(|_| {
            BCSError::Decoding(
                "AWS_SECRET_ACCESS_KEY is required when using the aws secret provider".to_string(),
            )
        })?;
        let region = std::env::var("AWS_REGION")
            .or_else(|_| std::env::var("AWS_DEFAULT_REGION"))
            .map_err(|_| {
                BCSError::Decoding(
                    "AWS_REGION or AWS_DEFAULT_REGION is required when using the aws secret provider"
                        .to_string(),
                )
            })?;

        if access_key.trim().is_empty() || secret_key.trim().is_empty() || region.trim().is_empty()
        {
            return Err(BCSError::Decoding(
                "AWS credentials and region must be non-empty".to_string(),
            ));
        }

        let session_token = std::env::var("AWS_SESSION_TOKEN")
            .ok()
            .filter(|s| !s.is_empty());
        let timeout_secs = std::env::var("BCS_AWS_SECRET_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
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

    /// Test helper with explicit settings and optional endpoint override.
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
        format!("https://secretsmanager.{}.amazonaws.com", self.region)
    }

    fn fetch_secret_string(&self, secret_id: &str) -> Result<String> {
        let endpoint = self.endpoint();
        let host = host_from_endpoint(&endpoint)?;
        let url = format!("{}/", endpoint.trim_end_matches('/'));
        let body = serde_json::json!({ "SecretId": secret_id }).to_string();
        let body_hash = hex::encode(Sha256::digest(body.as_bytes()));

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| BCSError::Decoding(format!("system clock error: {}", err)))?;
        let datetime = format_amz_datetime(now.as_secs());
        let date = &datetime[..8];

        let mut canonical_headers = format!(
            "content-type:application/x-amz-json-1.1\nhost:{}\nx-amz-date:{}\nx-amz-target:secretsmanager.GetSecretValue\n",
            host, datetime
        );
        let mut signed_headers = "content-type;host;x-amz-date;x-amz-target".to_string();
        if let Some(token) = &self.session_token {
            canonical_headers.push_str(&format!("x-amz-security-token:{}\n", token));
            signed_headers.push_str(";x-amz-security-token");
        }

        let canonical_request =
            format!("POST\n/\n\n{canonical_headers}\n{signed_headers}\n{body_hash}");
        let canonical_hash = hex::encode(Sha256::digest(canonical_request.as_bytes()));
        let credential_scope = format!("{}/{}/secretsmanager/aws4_request", date, self.region);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{}\n{}\n{}",
            datetime, credential_scope, canonical_hash
        );

        let signing_key = aws4_signing_key(&self.secret_key, date, &self.region, "secretsmanager");
        let signature = hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes()));
        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
            self.access_key, credential_scope, signed_headers, signature
        );

        let mut request = crate::http_util::agent(self.timeout)
            .post(&url)
            .header("content-type", "application/x-amz-json-1.1")
            .header("x-amz-target", "secretsmanager.GetSecretValue")
            .header("x-amz-date", &datetime)
            .header("authorization", &authorization);

        if let Some(token) = &self.session_token {
            request = request.header("x-amz-security-token", token);
        }

        let mut response = request
            .send(&body)
            .map_err(|err| map_ureq_error(secret_id, err))?;
        let status = response.status().as_u16();
        let resp_body = response.body_mut().read_to_string().map_err(|err| {
            BCSError::Decoding(format!(
                "Failed to read AWS Secrets Manager response for '{}': {}",
                secret_id, err
            ))
        })?;

        if !(200..300).contains(&status) {
            return Err(classify_aws_http_error(secret_id, status, &resp_body));
        }

        let json: serde_json::Value = serde_json::from_str(&resp_body).map_err(|err| {
            BCSError::Decoding(format!(
                "Failed to parse AWS Secrets Manager JSON for '{}': {}",
                secret_id, err
            ))
        })?;

        if let Some(s) = json.get("SecretString").and_then(|v| v.as_str()) {
            return Ok(s.to_string());
        }
        if json.get("SecretBinary").is_some() {
            return Err(BCSError::Decoding(format!(
                "AWS secret '{}' is binary; BCS secret refs require a string secret",
                secret_id
            )));
        }
        Err(BCSError::Decoding(format!(
            "AWS secret '{}' returned an empty payload",
            secret_id
        )))
    }
}

impl SecretResolver for AwsSecretResolver {
    fn resolve(&self, _scheme: &str, locator: &str) -> Result<String> {
        let (secret_id, field) = split_field(locator);
        if secret_id.is_empty() {
            return Err(BCSError::Decoding(
                "AWS secret locator must be non-empty (expected name/ARN or name#field)"
                    .to_string(),
            ));
        }

        let secret_string = self.fetch_secret_string(secret_id)?;

        if field.is_some() {
            let json: serde_json::Value = serde_json::from_str(&secret_string).map_err(|err| {
                BCSError::Decoding(format!(
                    "AWS secret '{}' is not JSON but locator requests a field: {}",
                    secret_id, err
                ))
            })?;
            return extract_secret_value(&json, field).map_err(|msg| {
                BCSError::Decoding(format!("Failed to resolve aws:'{}': {}", locator, msg))
            });
        }

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&secret_string) {
            match &json {
                serde_json::Value::String(s) => Ok(s.clone()),
                serde_json::Value::Object(map) if map.len() == 1 => {
                    extract_secret_value(&json, None).map_err(|msg| {
                        BCSError::Decoding(format!("Failed to resolve aws:'{}': {}", locator, msg))
                    })
                }
                serde_json::Value::Object(_) => Err(BCSError::Decoding(format!(
                    "Failed to resolve aws:'{}': secret is a multi-field JSON object; append #field",
                    locator
                ))),
                _ => Ok(secret_string),
            }
        } else {
            Ok(secret_string)
        }
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
    // UTC YYYYMMDD'T'HHMMSS'Z' without external time crate.
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

/// Howard Hinnant civil_from_days (UTC).
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

fn map_ureq_error(secret_id: &str, err: ureq::Error) -> BCSError {
    match err {
        ureq::Error::StatusCode(code) => classify_aws_http_error(secret_id, code, ""),
        _other => BCSError::Decoding(format!(
            "AWS Secrets Manager request for '{}' failed (unavailable)",
            secret_id
        )),
    }
}

fn classify_aws_http_error(secret_id: &str, status: u16, body: &str) -> BCSError {
    let lower = body.to_lowercase();
    let kind = if lower.contains("resourcenotfound") {
        "not found"
    } else if status == 403
        || lower.contains("accessdenied")
        || lower.contains("unrecognizedclient")
    {
        "denied"
    } else {
        "unavailable"
    };
    BCSError::Decoding(format!(
        "AWS Secrets Manager request for '{}' failed ({})",
        secret_id, kind
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn from_env_requires_credentials() {
        let had_key = std::env::var_os("AWS_ACCESS_KEY_ID");
        let had_secret = std::env::var_os("AWS_SECRET_ACCESS_KEY");
        let had_region = std::env::var_os("AWS_REGION");
        let had_default_region = std::env::var_os("AWS_DEFAULT_REGION");
        std::env::remove_var("AWS_ACCESS_KEY_ID");
        std::env::remove_var("AWS_SECRET_ACCESS_KEY");
        std::env::remove_var("AWS_REGION");
        std::env::remove_var("AWS_DEFAULT_REGION");
        let err = AwsSecretResolver::from_env().unwrap_err();
        assert!(err.to_string().contains("AWS_ACCESS_KEY_ID"));
        if let Some(v) = had_key {
            std::env::set_var("AWS_ACCESS_KEY_ID", v);
        }
        if let Some(v) = had_secret {
            std::env::set_var("AWS_SECRET_ACCESS_KEY", v);
        }
        if let Some(v) = had_region {
            std::env::set_var("AWS_REGION", v);
        }
        if let Some(v) = had_default_region {
            std::env::set_var("AWS_DEFAULT_REGION", v);
        }
    }

    #[test]
    fn resolve_against_mock_http_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = crate::http_util::read_http_request(&mut stream);
            assert!(
                request.contains("POST /") || request.starts_with("POST"),
                "unexpected request start: {}",
                &request[..request.len().min(80)]
            );
            assert!(request
                .to_ascii_lowercase()
                .contains("secretsmanager.getsecretvalue"));
            assert!(request.to_ascii_lowercase().contains("authorization:"));

            let body = r#"{"SecretString":"{\"password\":\"from-aws\"}"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/x-amz-json-1.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        });

        let resolver = AwsSecretResolver::new_for_test(
            "AKIATEST",
            "secret",
            "us-east-1",
            format!("http://{}", addr),
            Duration::from_secs(2),
        );
        let value = resolver.resolve("aws", "prod/db#password").unwrap();
        assert_eq!(value, "from-aws");
        server.join().unwrap();
    }

    #[test]
    fn amz_datetime_format_smoke() {
        // 2024-01-01 00:00:00 UTC
        assert_eq!(format_amz_datetime(1_704_067_200), "20240101T000000Z");
    }
}

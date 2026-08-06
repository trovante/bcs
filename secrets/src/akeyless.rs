//! Akeyless secret resolver.
//!
//! Auth (one of):
//! - `AKEYLESS_TOKEN` (already minted access token), or
//! - `AKEYLESS_ACCESS_ID` + `AKEYLESS_ACCESS_KEY` (auth-to-token)
//!
//! Optional: `AKEYLESS_API_URL` (default `https://api.akeyless.io`),
//! `BCS_AKEYLESS_TIMEOUT_SECS`.
//!
//! Locator (optional `#json_field`): secret path/name, e.g. `/prod/db/password`.

use crate::http_util::{finalize_optional_json_field, map_http_error};
use crate::locator::split_field;
use bcs_core::secret_resolver::SecretResolver;
use bcs_core::{BCSError, Result};
use std::time::Duration;

/// Resolves `akeyless:` secret references.
#[derive(Debug)]
pub struct AkeylessSecretResolver {
    token: String,
    api_url: String,
    timeout: Duration,
}

impl AkeylessSecretResolver {
    pub fn from_env() -> Result<Self> {
        let timeout_secs = std::env::var("BCS_AKEYLESS_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(10);
        let timeout = Duration::from_secs(timeout_secs.max(1));
        let api_url = std::env::var("AKEYLESS_API_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "https://api.akeyless.io".to_string());
        let api_url = api_url.trim_end_matches('/').to_string();

        let token = if let Ok(token) = std::env::var("AKEYLESS_TOKEN") {
            if token.trim().is_empty() {
                return Err(BCSError::Decoding(
                    "AKEYLESS_TOKEN must be non-empty".to_string(),
                ));
            }
            token
        } else {
            let access_id = std::env::var("AKEYLESS_ACCESS_ID").map_err(|_| missing_auth())?;
            let access_key = std::env::var("AKEYLESS_ACCESS_KEY").map_err(|_| missing_auth())?;
            if access_id.trim().is_empty() || access_key.trim().is_empty() {
                return Err(missing_auth());
            }
            auth_token(&api_url, &access_id, &access_key, timeout)?
        };

        Ok(Self {
            token,
            api_url,
            timeout,
        })
    }

    pub fn new_for_test(
        token: impl Into<String>,
        api_url: impl Into<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            token: token.into(),
            api_url: api_url.into().trim_end_matches('/').to_string(),
            timeout,
        }
    }

    fn fetch_secret_string(&self, name: &str) -> Result<String> {
        let url = format!("{}/get-secret-value", self.api_url);
        let body = serde_json::json!({
            "token": self.token,
            "names": [name],
        })
        .to_string();

        let response = ureq::post(&url)
            .timeout(self.timeout)
            .set("content-type", "application/json")
            .send_string(&body)
            .map_err(|err| map_http_error("Akeyless", name, err))?;

        let status = response.status();
        let resp_body = response.into_string().map_err(|err| {
            BCSError::Decoding(format!(
                "Failed to read Akeyless response for '{}': {}",
                name, err
            ))
        })?;
        if !(200..300).contains(&status) {
            return Err(crate::http_util::classify_status(
                "Akeyless", name, status, &resp_body,
            ));
        }

        let json: serde_json::Value = serde_json::from_str(&resp_body).map_err(|err| {
            BCSError::Decoding(format!(
                "Failed to parse Akeyless JSON for '{}': {}",
                name, err
            ))
        })?;

        // Response is typically a map of name -> value string.
        if let Some(s) = json.get(name).and_then(|v| v.as_str()) {
            return Ok(s.to_string());
        }
        if let Some(map) = json.as_object() {
            if map.len() == 1 {
                if let Some((_, v)) = map.iter().next() {
                    if let Some(s) = v.as_str() {
                        return Ok(s.to_string());
                    }
                }
            }
        }
        Err(BCSError::Decoding(format!(
            "Akeyless secret '{}' returned no string value",
            name
        )))
    }
}

impl SecretResolver for AkeylessSecretResolver {
    fn resolve(&self, _scheme: &str, locator: &str) -> Result<String> {
        let (resource, field) = split_field(locator);
        if resource.is_empty() {
            return Err(BCSError::Decoding(
                "Akeyless secret locator must be non-empty".to_string(),
            ));
        }
        let secret_string = self.fetch_secret_string(resource)?;
        finalize_optional_json_field("akeyless", locator, resource, &secret_string, field)
    }
}

fn missing_auth() -> BCSError {
    BCSError::Decoding(
        "Akeyless auth required: set AKEYLESS_TOKEN, or AKEYLESS_ACCESS_ID + AKEYLESS_ACCESS_KEY"
            .to_string(),
    )
}

fn auth_token(
    api_url: &str,
    access_id: &str,
    access_key: &str,
    timeout: Duration,
) -> Result<String> {
    let url = format!("{}/auth", api_url);
    let body = serde_json::json!({
        "access-id": access_id,
        "access-key": access_key,
    })
    .to_string();

    let response = ureq::post(&url)
        .timeout(timeout)
        .set("content-type", "application/json")
        .send_string(&body)
        .map_err(|_| {
            BCSError::Decoding("Akeyless auth request failed (unavailable)".to_string())
        })?;

    let status = response.status();
    let resp_body = response.into_string().map_err(|err| {
        BCSError::Decoding(format!("Failed to read Akeyless auth response: {}", err))
    })?;
    if !(200..300).contains(&status) {
        return Err(BCSError::Decoding(
            "Akeyless auth request failed (denied)".to_string(),
        ));
    }

    let json: serde_json::Value = serde_json::from_str(&resp_body).map_err(|err| {
        BCSError::Decoding(format!("Failed to parse Akeyless auth JSON: {}", err))
    })?;
    json.get("token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| BCSError::Decoding("Akeyless auth response missing token".to_string()))
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
            "AKEYLESS_TOKEN",
            "AKEYLESS_ACCESS_ID",
            "AKEYLESS_ACCESS_KEY",
        ];
        let saved: Vec<_> = keys.iter().map(|k| (*k, std::env::var_os(k))).collect();
        for k in &keys {
            std::env::remove_var(k);
        }
        let err = AkeylessSecretResolver::from_env().unwrap_err();
        assert!(err.to_string().contains("Akeyless auth required"));
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
            assert!(request.contains("POST /get-secret-value"));
            assert!(request.contains("prod"));
            assert!(request
                .to_ascii_lowercase()
                .contains("content-type: application/json"));

            let body = r#"{"/prod/db":"from-akeyless"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let resolver = AkeylessSecretResolver::new_for_test(
            "tok",
            format!("http://{}", addr),
            Duration::from_secs(2),
        );
        let value = resolver.resolve("akeyless", "/prod/db").unwrap();
        assert_eq!(value, "from-akeyless");
        server.join().unwrap();
    }
}

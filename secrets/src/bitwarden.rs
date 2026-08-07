//! Bitwarden Secrets Manager resolver.
//!
//! Auth (one of):
//! - `BWS_ACCESS_TOKEN` / `BITWARDEN_ACCESS_TOKEN` (machine account access token), or
//! - `BITWARDEN_CLIENT_ID` + `BITWARDEN_CLIENT_SECRET` (client credentials)
//!
//! Optional:
//! - `BITWARDEN_API_URL` (default `https://api.bitwarden.com`)
//! - `BITWARDEN_IDENTITY_URL` (default `https://identity.bitwarden.com`)
//! - `BCS_BITWARDEN_TIMEOUT_SECS`
//!
//! Locator (optional `#json_field`): secret UUID.

use crate::http_util::{finalize_optional_json_field, map_http_error};
use crate::locator::split_field;
use bcs_core::secret_resolver::SecretResolver;
use bcs_core::{BCSError, Result};
use std::time::Duration;

/// Resolves `bitwarden:` secret references.
#[derive(Debug)]
pub struct BitwardenSecretResolver {
    access_token: String,
    api_url: String,
    timeout: Duration,
}

impl BitwardenSecretResolver {
    pub fn from_env() -> Result<Self> {
        let timeout_secs = std::env::var("BCS_BITWARDEN_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(10);
        let timeout = Duration::from_secs(timeout_secs.max(1));

        let api_url = std::env::var("BITWARDEN_API_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "https://api.bitwarden.com".to_string());
        let api_url = api_url.trim_end_matches('/').to_string();

        let access_token = if let Ok(token) =
            std::env::var("BWS_ACCESS_TOKEN").or_else(|_| std::env::var("BITWARDEN_ACCESS_TOKEN"))
        {
            if token.trim().is_empty() {
                return Err(BCSError::Decoding(
                    "Bitwarden access token must be non-empty".to_string(),
                ));
            }
            token
        } else {
            let client_id = std::env::var("BITWARDEN_CLIENT_ID").map_err(|_| missing_auth())?;
            let client_secret =
                std::env::var("BITWARDEN_CLIENT_SECRET").map_err(|_| missing_auth())?;
            if client_id.trim().is_empty() || client_secret.trim().is_empty() {
                return Err(missing_auth());
            }
            let identity_url = std::env::var("BITWARDEN_IDENTITY_URL")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "https://identity.bitwarden.com".to_string());
            fetch_machine_token(
                identity_url.trim_end_matches('/'),
                &client_id,
                &client_secret,
                timeout,
            )?
        };

        Ok(Self {
            access_token,
            api_url,
            timeout,
        })
    }

    pub fn new_for_test(
        access_token: impl Into<String>,
        api_url: impl Into<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            access_token: access_token.into(),
            api_url: api_url.into().trim_end_matches('/').to_string(),
            timeout,
        }
    }

    fn fetch_secret_string(&self, secret_id: &str) -> Result<String> {
        let url = format!("{}/api/secrets/{}", self.api_url, secret_id);
        let mut response = crate::http_util::agent(self.timeout)
            .get(&url)
            .header("Authorization", &format!("Bearer {}", self.access_token))
            .header("accept", "application/json")
            .call()
            .map_err(|err| map_http_error("Bitwarden Secrets Manager", secret_id, err))?;

        let status = response.status().as_u16();
        let body = response.body_mut().read_to_string().map_err(|err| {
            BCSError::Decoding(format!(
                "Failed to read Bitwarden response for '{}': {}",
                secret_id, err
            ))
        })?;
        if !(200..300).contains(&status) {
            return Err(crate::http_util::classify_status(
                "Bitwarden Secrets Manager",
                secret_id,
                status,
                &body,
            ));
        }

        let json: serde_json::Value = serde_json::from_str(&body).map_err(|err| {
            BCSError::Decoding(format!(
                "Failed to parse Bitwarden JSON for '{}': {}",
                secret_id, err
            ))
        })?;

        json.get("value")
            .or_else(|| json.pointer("/data/value"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                BCSError::Decoding(format!(
                    "Bitwarden secret '{}' returned no string value",
                    secret_id
                ))
            })
    }
}

impl SecretResolver for BitwardenSecretResolver {
    fn resolve(&self, _scheme: &str, locator: &str) -> Result<String> {
        let (resource, field) = split_field(locator);
        if resource.is_empty() {
            return Err(BCSError::Decoding(
                "Bitwarden secret locator must be a non-empty secret id".to_string(),
            ));
        }
        let secret_string = self.fetch_secret_string(resource)?;
        finalize_optional_json_field("bitwarden", locator, resource, &secret_string, field)
    }
}

fn missing_auth() -> BCSError {
    BCSError::Decoding(
        "Bitwarden auth required: set BWS_ACCESS_TOKEN/BITWARDEN_ACCESS_TOKEN, or BITWARDEN_CLIENT_ID + BITWARDEN_CLIENT_SECRET"
            .to_string(),
    )
}

fn fetch_machine_token(
    identity_url: &str,
    client_id: &str,
    client_secret: &str,
    timeout: Duration,
) -> Result<String> {
    let url = format!("{}/connect/token", identity_url);
    let form = format!(
        "grant_type=client_credentials&scope=api.secrets&client_id={}&client_secret={}",
        urlencoding_lite(client_id),
        urlencoding_lite(client_secret)
    );

    let mut response = crate::http_util::agent(timeout)
        .post(&url)
        .header("content-type", "application/x-www-form-urlencoded")
        .send(&form)
        .map_err(|_| {
            BCSError::Decoding("Bitwarden token request failed (unavailable)".to_string())
        })?;

    let status = response.status().as_u16();
    let body = response.body_mut().read_to_string().map_err(|err| {
        BCSError::Decoding(format!("Failed to read Bitwarden token response: {}", err))
    })?;
    if !(200..300).contains(&status) {
        return Err(BCSError::Decoding(
            "Bitwarden token request failed (denied)".to_string(),
        ));
    }

    let json: serde_json::Value = serde_json::from_str(&body).map_err(|err| {
        BCSError::Decoding(format!("Failed to parse Bitwarden token JSON: {}", err))
    })?;
    json.get("access_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            BCSError::Decoding("Bitwarden token response missing access_token".to_string())
        })
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
    use std::io::Write;
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn from_env_requires_auth() {
        let keys = [
            "BWS_ACCESS_TOKEN",
            "BITWARDEN_ACCESS_TOKEN",
            "BITWARDEN_CLIENT_ID",
            "BITWARDEN_CLIENT_SECRET",
        ];
        let saved: Vec<_> = keys.iter().map(|k| (*k, std::env::var_os(k))).collect();
        for k in &keys {
            std::env::remove_var(k);
        }
        let err = BitwardenSecretResolver::from_env().unwrap_err();
        assert!(err.to_string().contains("Bitwarden auth required"));
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
            let request = crate::http_util::read_http_request(&mut stream);
            assert!(request.contains("GET /api/secrets/secret-uuid"));
            assert!(request
                .to_ascii_lowercase()
                .contains("authorization: bearer"));

            let body = r#"{"id":"secret-uuid","value":"from-bitwarden"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let resolver = BitwardenSecretResolver::new_for_test(
            "tok",
            format!("http://{}", addr),
            Duration::from_secs(2),
        );
        let value = resolver.resolve("bitwarden", "secret-uuid").unwrap();
        assert_eq!(value, "from-bitwarden");
        server.join().unwrap();
    }
}

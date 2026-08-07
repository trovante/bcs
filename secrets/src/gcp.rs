//! Google Cloud Secret Manager resolver (HTTP + bearer token).
//!
//! Auth (outside the BCS file):
//! - `GOOGLE_ACCESS_TOKEN` or `GCP_ACCESS_TOKEN` (required in phase 3b)
//! - `GOOGLE_CLOUD_PROJECT` or `GCP_PROJECT` — required for short secret-name locators
//! - `BCS_GCP_TIMEOUT_SECS` (optional, default 10)
//!
//! Locator forms (optional `#json_field`):
//! - Full resource: `projects/PROJECT/secrets/NAME/versions/latest`
//! - Short: `NAME` or `NAME/versions/N` (uses project from env)

use crate::http_util::{finalize_optional_json_field, map_http_error};
use crate::locator::split_field;
use bcs_core::secret_resolver::SecretResolver;
use bcs_core::{BCSError, Result};
use std::time::Duration;

/// Resolves `gcp:` secret references via Google Secret Manager.
#[derive(Debug)]
pub struct GcpSecretResolver {
    access_token: String,
    project: Option<String>,
    timeout: Duration,
    api_base: String,
}

impl GcpSecretResolver {
    /// Build from environment variables.
    pub fn from_env() -> Result<Self> {
        let access_token = std::env::var("GOOGLE_ACCESS_TOKEN")
            .or_else(|_| std::env::var("GCP_ACCESS_TOKEN"))
            .map_err(|_| {
                BCSError::Decoding(
                    "GOOGLE_ACCESS_TOKEN or GCP_ACCESS_TOKEN is required when using the gcp secret provider"
                        .to_string(),
                )
            })?;
        if access_token.trim().is_empty() {
            return Err(BCSError::Decoding(
                "GOOGLE_ACCESS_TOKEN / GCP_ACCESS_TOKEN must be non-empty".to_string(),
            ));
        }

        let project = std::env::var("GOOGLE_CLOUD_PROJECT")
            .or_else(|_| std::env::var("GCP_PROJECT"))
            .ok()
            .filter(|s| !s.is_empty());

        let timeout_secs = std::env::var("BCS_GCP_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(10);

        Ok(Self {
            access_token,
            project,
            timeout: Duration::from_secs(timeout_secs.max(1)),
            api_base: "https://secretmanager.googleapis.com".to_string(),
        })
    }

    /// Test helper.
    pub fn new_for_test(
        access_token: impl Into<String>,
        project: Option<String>,
        api_base: impl Into<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            access_token: access_token.into(),
            project,
            timeout,
            api_base: api_base.into().trim_end_matches('/').to_string(),
        }
    }

    fn resource_name(&self, resource: &str) -> Result<String> {
        if resource.starts_with("projects/") {
            return Ok(resource.to_string());
        }

        let project = self.project.as_deref().ok_or_else(|| {
            BCSError::Decoding(
                "GCP locator is a short secret name; set GOOGLE_CLOUD_PROJECT/GCP_PROJECT or use projects/.../secrets/.../versions/...".to_string(),
            )
        })?;

        if let Some((name, versions)) = resource.split_once("/versions/") {
            Ok(format!(
                "projects/{}/secrets/{}/versions/{}",
                project, name, versions
            ))
        } else {
            Ok(format!(
                "projects/{}/secrets/{}/versions/latest",
                project, resource
            ))
        }
    }

    fn fetch_secret_string(&self, resource: &str) -> Result<String> {
        let name = self.resource_name(resource)?;
        let url = format!("{}/v1/{}:access", self.api_base, name);

        let mut response = crate::http_util::agent(self.timeout)
            .get(&url)
            .header("Authorization", &format!("Bearer {}", self.access_token))
            .call()
            .map_err(|err| map_http_error("Google Secret Manager", resource, err))?;

        let status = response.status().as_u16();
        let body = response.body_mut().read_to_string().map_err(|err| {
            BCSError::Decoding(format!(
                "Failed to read Google Secret Manager response for '{}': {}",
                resource, err
            ))
        })?;

        if !(200..300).contains(&status) {
            return Err(crate::http_util::classify_status(
                "Google Secret Manager",
                resource,
                status,
                &body,
            ));
        }

        let json: serde_json::Value = serde_json::from_str(&body).map_err(|err| {
            BCSError::Decoding(format!(
                "Failed to parse Google Secret Manager JSON for '{}': {}",
                resource, err
            ))
        })?;

        let data_b64 = json
            .pointer("/payload/data")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                BCSError::Decoding(format!(
                    "Google Secret Manager secret '{}' returned no payload.data",
                    resource
                ))
            })?;

        decode_base64_secret(data_b64).map_err(|msg| {
            BCSError::Decoding(format!(
                "Failed to decode Google Secret Manager payload for '{}': {}",
                resource, msg
            ))
        })
    }
}

impl SecretResolver for GcpSecretResolver {
    fn resolve(&self, _scheme: &str, locator: &str) -> Result<String> {
        let (resource, field) = split_field(locator);
        if resource.is_empty() {
            return Err(BCSError::Decoding(
                "GCP secret locator must be non-empty".to_string(),
            ));
        }

        let secret_string = self.fetch_secret_string(resource)?;
        finalize_optional_json_field("gcp", locator, resource, &secret_string, field)
    }
}

fn decode_base64_secret(data: &str) -> std::result::Result<String, String> {
    // Secret Manager returns standard base64 (sometimes URL-safe). Try both.
    let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, data)
        .or_else(|_| {
            base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, data)
        })
        .or_else(|_| base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE, data))
        .map_err(|err| err.to_string())?;

    String::from_utf8(decoded).map_err(|err| format!("payload is not UTF-8 ({})", err))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn from_env_requires_token() {
        let had_google = std::env::var_os("GOOGLE_ACCESS_TOKEN");
        let had_gcp = std::env::var_os("GCP_ACCESS_TOKEN");
        std::env::remove_var("GOOGLE_ACCESS_TOKEN");
        std::env::remove_var("GCP_ACCESS_TOKEN");
        let err = GcpSecretResolver::from_env().unwrap_err();
        assert!(err.to_string().contains("ACCESS_TOKEN"));
        if let Some(v) = had_google {
            std::env::set_var("GOOGLE_ACCESS_TOKEN", v);
        }
        if let Some(v) = had_gcp {
            std::env::set_var("GCP_ACCESS_TOKEN", v);
        }
    }

    #[test]
    fn resource_name_short_and_full() {
        let resolver = GcpSecretResolver::new_for_test(
            "tok",
            Some("demo".into()),
            "http://example",
            Duration::from_secs(1),
        );
        assert_eq!(
            resolver.resource_name("db-pass").unwrap(),
            "projects/demo/secrets/db-pass/versions/latest"
        );
        assert_eq!(
            resolver.resource_name("db-pass/versions/3").unwrap(),
            "projects/demo/secrets/db-pass/versions/3"
        );
        assert_eq!(
            resolver
                .resource_name("projects/other/secrets/x/versions/latest")
                .unwrap(),
            "projects/other/secrets/x/versions/latest"
        );
    }

    #[test]
    fn resolve_against_mock_http_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let payload = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            br#"{"password":"from-gcp"}"#,
        );

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = crate::http_util::read_http_request(&mut stream);
            assert!(
                request.contains("GET /v1/projects/demo/secrets/db-pass/versions/latest:access")
            );
            assert!(request
                .to_ascii_lowercase()
                .contains("authorization: bearer"));

            let body = format!(r#"{{"payload":{{"data":"{}"}}}}"#, payload);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let resolver = GcpSecretResolver::new_for_test(
            "test-token",
            Some("demo".into()),
            format!("http://{}", addr),
            Duration::from_secs(2),
        );
        let value = resolver.resolve("gcp", "db-pass#password").unwrap();
        assert_eq!(value, "from-gcp");
        server.join().unwrap();
    }
}

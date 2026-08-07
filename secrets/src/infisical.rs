//! Infisical secret resolver.
//!
//! Auth: `INFISICAL_TOKEN` (bearer / service token).
//! Optional: `INFISICAL_API_URL` (default `https://app.infisical.com`),
//! `INFISICAL_PROJECT_ID` (or `INFISICAL_WORKSPACE_ID`),
//! `INFISICAL_ENVIRONMENT` (default `dev`),
//! `INFISICAL_SECRET_PATH` (default `/`),
//! `BCS_INFISICAL_TIMEOUT_SECS`.
//!
//! Locator (optional `#json_field`): secret name, e.g. `API_TOKEN`.

use crate::http_util::{finalize_optional_json_field, map_http_error};
use crate::locator::split_field;
use bcs_core::secret_resolver::SecretResolver;
use bcs_core::{BCSError, Result};
use std::time::Duration;

/// Resolves `infisical:` secret references.
#[derive(Debug)]
pub struct InfisicalSecretResolver {
    token: String,
    api_url: String,
    project_id: String,
    environment: String,
    secret_path: String,
    timeout: Duration,
}

impl InfisicalSecretResolver {
    pub fn from_env() -> Result<Self> {
        let token = std::env::var("INFISICAL_TOKEN").map_err(|_| {
            BCSError::Decoding(
                "INFISICAL_TOKEN is required when using the infisical secret provider".to_string(),
            )
        })?;
        let project_id = std::env::var("INFISICAL_PROJECT_ID")
            .or_else(|_| std::env::var("INFISICAL_WORKSPACE_ID"))
            .map_err(|_| {
                BCSError::Decoding(
                    "INFISICAL_PROJECT_ID (or INFISICAL_WORKSPACE_ID) is required when using the infisical secret provider".to_string(),
                )
            })?;
        if token.trim().is_empty() || project_id.trim().is_empty() {
            return Err(BCSError::Decoding(
                "INFISICAL_TOKEN and INFISICAL_PROJECT_ID must be non-empty".to_string(),
            ));
        }

        let api_url = std::env::var("INFISICAL_API_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "https://app.infisical.com".to_string());
        let environment = std::env::var("INFISICAL_ENVIRONMENT")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "dev".to_string());
        let secret_path = std::env::var("INFISICAL_SECRET_PATH")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "/".to_string());
        let timeout_secs = std::env::var("BCS_INFISICAL_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(10);

        Ok(Self {
            token,
            api_url: api_url.trim_end_matches('/').to_string(),
            project_id,
            environment,
            secret_path,
            timeout: Duration::from_secs(timeout_secs.max(1)),
        })
    }

    pub fn new_for_test(
        token: impl Into<String>,
        api_url: impl Into<String>,
        project_id: impl Into<String>,
        environment: impl Into<String>,
        secret_path: impl Into<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            token: token.into(),
            api_url: api_url.into().trim_end_matches('/').to_string(),
            project_id: project_id.into(),
            environment: environment.into(),
            secret_path: secret_path.into(),
            timeout,
        }
    }

    fn fetch_secret_string(&self, name: &str) -> Result<String> {
        let url = format!(
            "{}/api/v3/secrets/raw/{}?workspaceId={}&environment={}&secretPath={}",
            self.api_url,
            urlencoding_lite(name),
            urlencoding_lite(&self.project_id),
            urlencoding_lite(&self.environment),
            urlencoding_lite(&self.secret_path)
        );

        let mut response = crate::http_util::agent(self.timeout)
            .get(&url)
            .header("Authorization", &format!("Bearer {}", self.token))
            .header("accept", "application/json")
            .call()
            .map_err(|err| map_http_error("Infisical", name, err))?;

        let status = response.status().as_u16();
        let body = response.body_mut().read_to_string().map_err(|err| {
            BCSError::Decoding(format!(
                "Failed to read Infisical response for '{}': {}",
                name, err
            ))
        })?;
        if !(200..300).contains(&status) {
            return Err(crate::http_util::classify_status(
                "Infisical",
                name,
                status,
                &body,
            ));
        }

        let json: serde_json::Value = serde_json::from_str(&body).map_err(|err| {
            BCSError::Decoding(format!(
                "Failed to parse Infisical JSON for '{}': {}",
                name, err
            ))
        })?;

        json.pointer("/secret/secretValue")
            .or_else(|| json.pointer("/secretValue"))
            .or_else(|| json.get("value"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                BCSError::Decoding(format!(
                    "Infisical secret '{}' returned no string value",
                    name
                ))
            })
    }
}

impl SecretResolver for InfisicalSecretResolver {
    fn resolve(&self, _scheme: &str, locator: &str) -> Result<String> {
        let (resource, field) = split_field(locator);
        if resource.is_empty() {
            return Err(BCSError::Decoding(
                "Infisical secret locator must be non-empty".to_string(),
            ));
        }
        let secret_string = self.fetch_secret_string(resource)?;
        finalize_optional_json_field("infisical", locator, resource, &secret_string, field)
    }
}

fn urlencoding_lite(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
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
    fn from_env_requires_token_and_project() {
        let had_token = std::env::var_os("INFISICAL_TOKEN");
        let had_proj = std::env::var_os("INFISICAL_PROJECT_ID");
        let had_ws = std::env::var_os("INFISICAL_WORKSPACE_ID");
        std::env::remove_var("INFISICAL_TOKEN");
        std::env::remove_var("INFISICAL_PROJECT_ID");
        std::env::remove_var("INFISICAL_WORKSPACE_ID");
        let err = InfisicalSecretResolver::from_env().unwrap_err();
        assert!(err.to_string().contains("INFISICAL_TOKEN"));
        if let Some(v) = had_token {
            std::env::set_var("INFISICAL_TOKEN", v);
        }
        if let Some(v) = had_proj {
            std::env::set_var("INFISICAL_PROJECT_ID", v);
        }
        if let Some(v) = had_ws {
            std::env::set_var("INFISICAL_WORKSPACE_ID", v);
        }
    }

    #[test]
    fn resolve_against_mock_http_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = crate::http_util::read_http_request(&mut stream);
            assert!(request.contains("GET /api/v3/secrets/raw/API_TOKEN?"));
            assert!(request.contains("workspaceId=ws1"));

            let body = r#"{"secret":{"secretKey":"API_TOKEN","secretValue":"from-infisical"}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let resolver = InfisicalSecretResolver::new_for_test(
            "tok",
            format!("http://{}", addr),
            "ws1",
            "dev",
            "/",
            Duration::from_secs(2),
        );
        let value = resolver.resolve("infisical", "API_TOKEN").unwrap();
        assert_eq!(value, "from-infisical");
        server.join().unwrap();
    }
}

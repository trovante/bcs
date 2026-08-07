//! Doppler secret resolver.
//!
//! Auth: `DOPPLER_TOKEN` (service token / personal token).
//! Optional: `DOPPLER_API_URL` (default `https://api.doppler.com`),
//! `DOPPLER_PROJECT`, `DOPPLER_CONFIG`, `BCS_DOPPLER_TIMEOUT_SECS`.
//!
//! Locator (optional `#json_field`):
//! - `SECRET_NAME` (uses `DOPPLER_PROJECT` + `DOPPLER_CONFIG`)
//! - `project/config/SECRET_NAME`

use crate::http_util::{finalize_optional_json_field, map_http_error};
use crate::locator::split_field;
use bcs_core::secret_resolver::SecretResolver;
use bcs_core::{BCSError, Result};
use std::time::Duration;

/// Resolves `doppler:` secret references.
#[derive(Debug)]
pub struct DopplerSecretResolver {
    token: String,
    api_url: String,
    default_project: Option<String>,
    default_config: Option<String>,
    timeout: Duration,
}

impl DopplerSecretResolver {
    pub fn from_env() -> Result<Self> {
        let token = std::env::var("DOPPLER_TOKEN").map_err(|_| {
            BCSError::Decoding(
                "DOPPLER_TOKEN is required when using the doppler secret provider".to_string(),
            )
        })?;
        if token.trim().is_empty() {
            return Err(BCSError::Decoding(
                "DOPPLER_TOKEN must be non-empty".to_string(),
            ));
        }

        let api_url = std::env::var("DOPPLER_API_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "https://api.doppler.com".to_string());
        let timeout_secs = std::env::var("BCS_DOPPLER_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(10);

        Ok(Self {
            token,
            api_url: api_url.trim_end_matches('/').to_string(),
            default_project: std::env::var("DOPPLER_PROJECT")
                .ok()
                .filter(|s| !s.is_empty()),
            default_config: std::env::var("DOPPLER_CONFIG")
                .ok()
                .filter(|s| !s.is_empty()),
            timeout: Duration::from_secs(timeout_secs.max(1)),
        })
    }

    pub fn new_for_test(
        token: impl Into<String>,
        api_url: impl Into<String>,
        project: Option<String>,
        config: Option<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            token: token.into(),
            api_url: api_url.into().trim_end_matches('/').to_string(),
            default_project: project,
            default_config: config,
            timeout,
        }
    }

    fn resolve_coords(&self, resource: &str) -> Result<(String, String, String)> {
        let parts: Vec<&str> = resource.splitn(3, '/').collect();
        match parts.as_slice() {
            [name] => {
                let project = self.default_project.clone().ok_or_else(|| {
                    BCSError::Decoding(
                        "Doppler locator is a bare name; set DOPPLER_PROJECT/DOPPLER_CONFIG or use project/config/name".to_string(),
                    )
                })?;
                let config = self.default_config.clone().ok_or_else(|| {
                    BCSError::Decoding(
                        "Doppler locator is a bare name; set DOPPLER_PROJECT/DOPPLER_CONFIG or use project/config/name".to_string(),
                    )
                })?;
                Ok((project, config, (*name).to_string()))
            }
            [project, config, name]
                if !project.is_empty() && !config.is_empty() && !name.is_empty() =>
            {
                Ok((
                    (*project).to_string(),
                    (*config).to_string(),
                    (*name).to_string(),
                ))
            }
            _ => Err(BCSError::Decoding(
                "Doppler locator must be SECRET_NAME or project/config/SECRET_NAME".to_string(),
            )),
        }
    }

    fn fetch_secret_string(&self, resource: &str) -> Result<String> {
        let (project, config, name) = self.resolve_coords(resource)?;
        let url = format!(
            "{}/v3/configs/config/secret?project={}&config={}&name={}",
            self.api_url,
            urlencoding_lite(&project),
            urlencoding_lite(&config),
            urlencoding_lite(&name)
        );

        let mut response = crate::http_util::agent(self.timeout)
            .get(&url)
            .header("Authorization", &format!("Bearer {}", self.token))
            .header("accept", "application/json")
            .call()
            .map_err(|err| map_http_error("Doppler", resource, err))?;

        let status = response.status().as_u16();
        let body = response.body_mut().read_to_string().map_err(|err| {
            BCSError::Decoding(format!(
                "Failed to read Doppler response for '{}': {}",
                resource, err
            ))
        })?;
        if !(200..300).contains(&status) {
            return Err(crate::http_util::classify_status(
                "Doppler", resource, status, &body,
            ));
        }

        let json: serde_json::Value = serde_json::from_str(&body).map_err(|err| {
            BCSError::Decoding(format!(
                "Failed to parse Doppler JSON for '{}': {}",
                resource, err
            ))
        })?;

        json.pointer("/value/computed")
            .or_else(|| json.pointer("/value/raw"))
            .or_else(|| json.get("value"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                BCSError::Decoding(format!(
                    "Doppler secret '{}' returned no string value",
                    resource
                ))
            })
    }
}

impl SecretResolver for DopplerSecretResolver {
    fn resolve(&self, _scheme: &str, locator: &str) -> Result<String> {
        let (resource, field) = split_field(locator);
        if resource.is_empty() {
            return Err(BCSError::Decoding(
                "Doppler secret locator must be non-empty".to_string(),
            ));
        }
        let secret_string = self.fetch_secret_string(resource)?;
        finalize_optional_json_field("doppler", locator, resource, &secret_string, field)
    }
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
    fn from_env_requires_token() {
        let had = std::env::var_os("DOPPLER_TOKEN");
        std::env::remove_var("DOPPLER_TOKEN");
        let err = DopplerSecretResolver::from_env().unwrap_err();
        assert!(err.to_string().contains("DOPPLER_TOKEN"));
        if let Some(v) = had {
            std::env::set_var("DOPPLER_TOKEN", v);
        }
    }

    #[test]
    fn resolve_against_mock_http_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = crate::http_util::read_http_request(&mut stream);
            assert!(request.contains("GET /v3/configs/config/secret?"));
            assert!(request.contains("project=proj"));
            assert!(request.contains("config=dev"));
            assert!(request.contains("name=API_TOKEN"));

            let body =
                r#"{"name":"API_TOKEN","value":{"raw":"from-doppler","computed":"from-doppler"}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let resolver = DopplerSecretResolver::new_for_test(
            "dp.token",
            format!("http://{}", addr),
            None,
            None,
            Duration::from_secs(2),
        );
        let value = resolver.resolve("doppler", "proj/dev/API_TOKEN").unwrap();
        assert_eq!(value, "from-doppler");
        server.join().unwrap();
    }
}

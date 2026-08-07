//! Small HTTP helpers shared by remote secret providers (ureq 3).

use crate::locator::extract_secret_value;
use bcs_core::{BCSError, Result};
use std::time::Duration;
use ureq::Agent;

/// Build an agent that returns HTTP error responses instead of `Error::StatusCode`,
/// matching the ureq 2 call-site pattern of inspecting `status` + body.
pub fn agent(timeout: Duration) -> Agent {
    Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build()
        .into()
}

pub fn finalize_optional_json_field(
    scheme: &str,
    locator: &str,
    resource: &str,
    secret_string: &str,
    field: Option<&str>,
) -> Result<String> {
    if field.is_some() {
        let json: serde_json::Value = serde_json::from_str(secret_string).map_err(|err| {
            BCSError::Decoding(format!(
                "{} secret '{}' is not JSON but locator requests a field: {}",
                scheme, resource, err
            ))
        })?;
        return extract_secret_value(&json, field).map_err(|msg| {
            BCSError::Decoding(format!(
                "Failed to resolve {}:'{}': {}",
                scheme, locator, msg
            ))
        });
    }

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(secret_string) {
        match &json {
            serde_json::Value::String(s) => Ok(s.clone()),
            serde_json::Value::Object(map) if map.len() == 1 => extract_secret_value(&json, None)
                .map_err(|msg| {
                    BCSError::Decoding(format!(
                        "Failed to resolve {}:'{}': {}",
                        scheme, locator, msg
                    ))
                }),
            serde_json::Value::Object(_) => Err(BCSError::Decoding(format!(
                "Failed to resolve {}:'{}': secret is a multi-field JSON object; append #field",
                scheme, locator
            ))),
            _ => Ok(secret_string.to_string()),
        }
    } else {
        Ok(secret_string.to_string())
    }
}

pub fn map_http_error(provider: &str, resource: &str, err: ureq::Error) -> BCSError {
    match err {
        ureq::Error::StatusCode(code) => classify_status(provider, resource, code, ""),
        _ => BCSError::Decoding(format!(
            "{} request for '{}' failed (unavailable)",
            provider, resource
        )),
    }
}

pub fn classify_status(provider: &str, resource: &str, status: u16, body: &str) -> BCSError {
    let lower = body.to_lowercase();
    let kind = if status == 404 || lower.contains("secretnotfound") || lower.contains("not_found") {
        "not found"
    } else if status == 401
        || status == 403
        || lower.contains("unauthorized")
        || lower.contains("permissiondenied")
    {
        "denied"
    } else {
        "unavailable"
    };
    BCSError::Decoding(format!(
        "{} request for '{}' failed ({})",
        provider, resource, kind
    ))
}

pub fn status_only(err: ureq::Error) -> String {
    match err {
        ureq::Error::StatusCode(code) => format!("HTTP {}", code),
        other => other.to_string(),
    }
}

/// Read a full HTTP/1.1 request (headers + Content-Length body) from a mock TCP stream.
///
/// ureq 3 may deliver headers and body in separate packets; a single `read` is not enough
/// for POST mocks that assert on the JSON body.
#[cfg(test)]
pub fn read_http_request(stream: &mut impl std::io::Read) -> String {
    let mut data = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = stream.read(&mut buf).unwrap_or(0);
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
        if data.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }

    let header_end = data
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| i + 4)
        .unwrap_or(data.len());
    let headers = String::from_utf8_lossy(&data[..header_end]);
    let content_length = headers.lines().find_map(|line| {
        let lower = line.to_ascii_lowercase();
        lower
            .strip_prefix("content-length:")
            .and_then(|v| v.trim().parse::<usize>().ok())
    });

    if let Some(len) = content_length {
        while data.len() < header_end + len {
            let n = stream.read(&mut buf).unwrap_or(0);
            if n == 0 {
                break;
            }
            data.extend_from_slice(&buf[..n]);
        }
    }

    String::from_utf8_lossy(&data).into_owned()
}

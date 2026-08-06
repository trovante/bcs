//! Small HTTP helpers shared by remote secret providers.

use crate::locator::extract_secret_value;
use bcs_core::{BCSError, Result};

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
        ureq::Error::Status(code, response) => {
            let body = response.into_string().unwrap_or_default();
            classify_status(provider, resource, code, &body)
        }
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

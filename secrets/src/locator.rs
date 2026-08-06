//! Locator parsing shared by secret providers.
//!
//! Format: `path_or_name` or `path_or_name#json_field`.

/// Split `locator` into `(resource, optional_json_field)`.
pub fn split_field(locator: &str) -> (&str, Option<&str>) {
    match locator.split_once('#') {
        Some((resource, field)) if !resource.is_empty() && !field.is_empty() => {
            (resource, Some(field))
        }
        _ => (locator, None),
    }
}

/// Extract a string value from a JSON document, optionally at `field`.
///
/// - With `field`: reads object key (stringifies primitives).
/// - Without `field`: accepts a JSON string, or a single-key object whose value
///   is a string/primitive; otherwise errors asking for `#field`.
pub fn extract_secret_value(
    value: &serde_json::Value,
    field: Option<&str>,
) -> Result<String, String> {
    if let Some(field_name) = field {
        let obj = value.as_object().ok_or_else(|| {
            format!(
                "secret payload is not a JSON object; cannot read field '{}'",
                field_name
            )
        })?;
        let entry = obj
            .get(field_name)
            .ok_or_else(|| format!("secret payload does not contain field '{}'", field_name))?;
        return json_to_string(entry, field_name);
    }

    match value {
        serde_json::Value::String(s) => Ok(s.clone()),
        serde_json::Value::Object(map) if map.len() == 1 => {
            let (key, entry) = map.iter().next().expect("len == 1");
            json_to_string(entry, key)
        }
        serde_json::Value::Object(_) => Err(
            "secret payload is a multi-field JSON object; append #field to the locator".to_string(),
        ),
        other => json_to_string(other, "value"),
    }
}

fn json_to_string(value: &serde_json::Value, label: &str) -> Result<String, String> {
    match value {
        serde_json::Value::String(s) => Ok(s.clone()),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        serde_json::Value::Bool(b) => Ok(b.to_string()),
        serde_json::Value::Null => Err(format!("secret field '{}' is null", label)),
        _ => Err(format!(
            "secret field '{}' is not a scalar value (string/number/bool)",
            label
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn split_field_basic() {
        assert_eq!(split_field("a/b"), ("a/b", None));
        assert_eq!(split_field("a/b#pass"), ("a/b", Some("pass")));
        assert_eq!(split_field("#x"), ("#x", None));
        assert_eq!(split_field("a#"), ("a#", None));
    }

    #[test]
    fn extract_with_and_without_field() {
        let obj = json!({"password": "s3cret", "user": "admin"});
        assert_eq!(
            extract_secret_value(&obj, Some("password")).unwrap(),
            "s3cret"
        );
        assert!(extract_secret_value(&obj, None)
            .unwrap_err()
            .contains("multi-field"));

        let single = json!({"token": "abc"});
        assert_eq!(extract_secret_value(&single, None).unwrap(), "abc");

        let plain = json!("raw");
        assert_eq!(extract_secret_value(&plain, None).unwrap(), "raw");
    }
}

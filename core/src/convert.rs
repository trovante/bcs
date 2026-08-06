use crate::error::{BCSError, Result};
use crate::limits;
use crate::types::Value;
use base64::Engine;

/// Convert BCS Value into serde_json::Value.
///
/// Bytes are encoded as base64 strings.
pub fn value_to_json(value: &Value) -> Result<serde_json::Value> {
    value_to_json_at(value, 0)
}

fn value_to_json_at(value: &Value, depth: usize) -> Result<serde_json::Value> {
    limits::ensure_depth(depth)?;
    match value {
        Value::Null => Ok(serde_json::Value::Null),
        Value::Bool(b) => Ok(serde_json::Value::Bool(*b)),
        Value::Int8(i) => Ok(serde_json::Value::Number((*i as i64).into())),
        Value::Int16(i) => Ok(serde_json::Value::Number((*i as i64).into())),
        Value::Int32(i) => Ok(serde_json::Value::Number((*i as i64).into())),
        Value::Int64(i) => Ok(serde_json::Value::Number((*i).into())),
        Value::UInt8(u) => Ok(serde_json::Value::Number((*u as u64).into())),
        Value::UInt16(u) => Ok(serde_json::Value::Number((*u as u64).into())),
        Value::UInt32(u) => Ok(serde_json::Value::Number((*u as u64).into())),
        Value::UInt64(u) => Ok(serde_json::Value::Number((*u).into())),
        Value::Float32(f) => serde_json::Number::from_f64(*f as f64)
            .map(serde_json::Value::Number)
            .ok_or_else(|| BCSError::Encoding("Invalid float value".to_string())),
        Value::Float64(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .ok_or_else(|| BCSError::Encoding("Invalid float value".to_string())),
        Value::String(s) => Ok(serde_json::Value::String(s.clone())),
        Value::Bytes(b) => Ok(serde_json::Value::String(
            base64::engine::general_purpose::STANDARD.encode(b),
        )),
        Value::List(items) => {
            let mut json_items = Vec::with_capacity(items.len());
            for item in items {
                json_items.push(value_to_json_at(item, depth + 1)?);
            }
            Ok(serde_json::Value::Array(json_items))
        }
        Value::Map(entries) => {
            let mut map = serde_json::Map::new();
            for (key, val) in entries {
                let key_str = match key {
                    Value::String(s) => s.clone(),
                    _ => format!("{:?}", key),
                };
                map.insert(key_str, value_to_json_at(val, depth + 1)?);
            }
            Ok(serde_json::Value::Object(map))
        }
        Value::Struct(fields) => {
            let mut map = serde_json::Map::new();
            for (field_name, _hash, val) in fields {
                map.insert(field_name.clone(), value_to_json_at(val, depth + 1)?);
            }
            Ok(serde_json::Value::Object(map))
        }
        Value::Union(tag, val) => {
            let mut map = serde_json::Map::new();
            map.insert("tag".to_string(), serde_json::Value::Number((*tag).into()));
            map.insert("value".to_string(), value_to_json_at(val, depth + 1)?);
            Ok(serde_json::Value::Object(map))
        }
        Value::Optional(opt) => match opt {
            Some(val) => value_to_json_at(val, depth + 1),
            None => Ok(serde_json::Value::Null),
        },
    }
}

//! Inspect AST over the BCS data layer (ops DX; not a second wire format).
//!
//! Prefer the **offset cursor** path ([`InspectNode::from_decoder`]): indexed roots
//! list top-level fields without decoding values; nested containers walk the wire
//! with [`Decoder::skip_value`]. [`InspectNode::from_value`] remains for tests and
//! fallback when a tag cannot be walked.

use crate::decoder::Decoder;
use crate::error::{BCSError, Result};
use crate::security::{PREFIX_KMS, PREFIX_PBKDF2};
use crate::string_table::StringTable;
use crate::types::{CompositeDecoder, TypeTag, Value};
use std::io::{Cursor, Read};
use std::sync::Arc;

const SECRET_REF_PREFIX: &str = "__bcs_secret_ref__:";

/// Lazy node describing a value (masking-aware AST for ops DX).
#[derive(Debug, Clone)]
pub struct InspectNode {
    pub path: String,
    pub type_name: String,
    pub offset: Option<u64>,
    pub marker: Option<InspectMarker>,
    pub preview: Option<String>,
    backend: InspectBackend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectMarker {
    Protected,
    SecretRef,
}

#[derive(Debug, Clone)]
enum InspectBackend {
    /// Children already decoded as [`Value`] (tests / fallback).
    Value { children: Vec<(String, Value)> },
    /// Offset-backed children into a shared logical data layer.
    Cursor {
        data: Arc<[u8]>,
        string_table: Option<Arc<StringTable>>,
        children: Vec<(String, u64)>,
    },
}

impl InspectNode {
    /// Build a root inspect node using the offset cursor when possible.
    ///
    /// Indexed files: root children come from the index table (no value decode).
    /// Compact / no-index: walk the single root value at data-layer offset 0.
    pub fn from_decoder(decoder: &mut Decoder) -> Result<Self> {
        let data = decoder.logical_data_layer()?;
        let string_table = decoder.string_table()?;
        let entries = decoder.top_level_index_entries()?;

        if !entries.is_empty() {
            let children: Vec<(String, u64)> = entries;
            return Ok(Self {
                path: String::new(),
                type_name: "struct".into(),
                offset: None,
                marker: None,
                preview: Some(format!("{{{} fields}}", children.len())),
                backend: InspectBackend::Cursor {
                    data,
                    string_table,
                    children,
                },
            });
        }

        if data.is_empty() {
            return Ok(Self::from_value("", &Value::Null, Some(0)));
        }

        Self::from_offset("", 0, data, string_table)
    }

    /// Build a node by decoding a [`Value`] (tests and fallback).
    pub fn from_value(path: &str, value: &Value, offset: Option<u64>) -> Self {
        let marker = if is_protected_string_value(value) {
            Some(InspectMarker::Protected)
        } else if is_secret_ref_string_value(value) {
            Some(InspectMarker::SecretRef)
        } else {
            None
        };

        let type_name = type_name_of(value);
        let preview = match marker {
            Some(InspectMarker::Protected) => Some("[PROTECTED]".into()),
            Some(InspectMarker::SecretRef) => Some("[SECRET_REF]".into()),
            None => leaf_preview(value),
        };

        let children = match value {
            Value::Struct(fields) if marker.is_none() => fields
                .iter()
                .map(|(name, _, child)| (name.clone(), child.clone()))
                .collect(),
            Value::List(items) if marker.is_none() => items
                .iter()
                .enumerate()
                .map(|(i, child)| (format!("[{}]", i), child.clone()))
                .collect(),
            Value::Map(entries) if marker.is_none() => entries
                .iter()
                .map(|(k, child)| {
                    let name = match k {
                        Value::String(s) => s.clone(),
                        other => format!("{:?}", other),
                    };
                    (name, child.clone())
                })
                .collect(),
            _ => Vec::new(),
        };

        Self {
            path: path.to_string(),
            type_name,
            offset,
            marker,
            preview,
            backend: InspectBackend::Value { children },
        }
    }

    fn from_offset(
        path: &str,
        offset: u64,
        data: Arc<[u8]>,
        string_table: Option<Arc<StringTable>>,
    ) -> Result<Self> {
        let off = offset as usize;
        if off >= data.len() {
            return Err(BCSError::Decoding("inspect offset out of range".into()));
        }

        let tag = peek_type_tag(&data, off)?;
        let type_name = type_name_of_tag(tag).to_string();

        match tag {
            TypeTag::List | TypeTag::Map | TypeTag::Struct => {
                let children = enumerate_children_at(&data, offset, string_table.clone())?;
                let preview = Some(match tag {
                    TypeTag::List => format!("[{} items]", children.len()),
                    TypeTag::Map => format!("{{{} entries}}", children.len()),
                    TypeTag::Struct => format!("{{{} fields}}", children.len()),
                    _ => unreachable!(),
                });
                Ok(Self {
                    path: path.to_string(),
                    type_name,
                    offset: Some(offset),
                    marker: None,
                    preview,
                    backend: InspectBackend::Cursor {
                        data,
                        string_table,
                        children,
                    },
                })
            }
            TypeTag::OptionalSome => {
                // Single child at payload after tag.
                let child_off = offset + 1;
                Ok(Self {
                    path: path.to_string(),
                    type_name: "optional".into(),
                    offset: Some(offset),
                    marker: None,
                    preview: None,
                    backend: InspectBackend::Cursor {
                        data,
                        string_table,
                        children: vec![("some".into(), child_off)],
                    },
                })
            }
            TypeTag::Union => {
                // variant u32 + value — expose value as child
                let child_off = offset + 1 + 4;
                Ok(Self {
                    path: path.to_string(),
                    type_name: "union".into(),
                    offset: Some(offset),
                    marker: None,
                    preview: None,
                    backend: InspectBackend::Cursor {
                        data,
                        string_table,
                        children: vec![("value".into(), child_off)],
                    },
                })
            }
            _ => {
                // Leaf: decode only this value for preview/markers.
                let mut decoder = CompositeDecoder::new();
                if let Some(table) = &string_table {
                    decoder = decoder.with_string_table(table.clone());
                }
                let mut cursor = Cursor::new(&data[off..]);
                let value = decoder.decode_value(&mut cursor)?;
                Ok(Self::from_value(path, &value, Some(offset)))
            }
        }
    }

    pub fn child_count(&self) -> usize {
        match &self.backend {
            InspectBackend::Value { children } => children.len(),
            InspectBackend::Cursor { children, .. } => children.len(),
        }
    }

    /// Materialize immediate children (one level).
    pub fn children(&self) -> Result<Vec<InspectNode>> {
        match &self.backend {
            InspectBackend::Value { children } => Ok(children
                .iter()
                .map(|(name, value)| {
                    let child_path = join_path(&self.path, name);
                    Self::from_value(&child_path, value, None)
                })
                .collect()),
            InspectBackend::Cursor {
                data,
                string_table,
                children,
            } => {
                let mut out = Vec::with_capacity(children.len());
                for (name, offset) in children {
                    let child_path = join_path(&self.path, name);
                    out.push(Self::from_offset(
                        &child_path,
                        *offset,
                        data.clone(),
                        string_table.clone(),
                    )?);
                }
                Ok(out)
            }
        }
    }

    /// Render a debug tree (protect/secret leaves stay masked).
    pub fn format_tree(&self) -> Result<String> {
        let mut out = String::new();
        self.write_tree(&mut out, "", true)?;
        Ok(out)
    }

    fn write_tree(&self, out: &mut String, indent: &str, last: bool) -> Result<()> {
        let branch = if indent.is_empty() {
            ""
        } else if last {
            "└─ "
        } else {
            "├─ "
        };
        let label = if self.path.is_empty() {
            "<root>".to_string()
        } else {
            self.path
                .rsplit(['.', '['])
                .next()
                .unwrap_or(&self.path)
                .trim_end_matches(']')
                .to_string()
        };
        if let Some(preview) = &self.preview {
            out.push_str(&format!(
                "{}{}{}: {} ({})\n",
                indent, branch, label, preview, self.type_name
            ));
        } else {
            out.push_str(&format!(
                "{}{}{} ({})\n",
                indent, branch, label, self.type_name
            ));
        }
        let next_indent = if indent.is_empty() {
            String::new()
        } else {
            format!("{}{}", indent, if last { "   " } else { "│  " })
        };
        let kids = self.children()?;
        let count = kids.len();
        for (i, child) in kids.into_iter().enumerate() {
            let child_indent = if indent.is_empty() {
                "  "
            } else {
                &next_indent
            };
            child.write_tree(out, child_indent, i + 1 == count)?;
        }
        Ok(())
    }
}

fn join_path(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else if name.starts_with('[') {
        format!("{}{}", parent, name)
    } else {
        format!("{}.{}", parent, name)
    }
}

fn enumerate_children_at(
    data: &[u8],
    offset: u64,
    string_table: Option<Arc<StringTable>>,
) -> Result<Vec<(String, u64)>> {
    let start = offset as usize;
    let mut cursor = Cursor::new(&data[start..]);
    let mut tag_buf = [0u8; 1];
    cursor.read_exact(&mut tag_buf)?;
    let tag = TypeTag::from_u8(tag_buf[0])?;

    let mut decoder = CompositeDecoder::new();
    if let Some(table) = &string_table {
        decoder = decoder.with_string_table(table.clone());
    }

    match tag {
        TypeTag::Struct => {
            let mut count_buf = [0u8; 4];
            cursor.read_exact(&mut count_buf)?;
            let count = u32::from_le_bytes(count_buf) as usize;
            let mut children = Vec::with_capacity(count);
            for _ in 0..count {
                let name_val = decoder.decode_value(&mut cursor)?;
                let name = match name_val {
                    Value::String(s) => s,
                    _ => {
                        return Err(BCSError::Decoding(
                            "Invalid struct field name encoding".into(),
                        ))
                    }
                };
                let mut hash_buf = [0u8; 8];
                cursor.read_exact(&mut hash_buf)?;
                let value_off = offset + cursor.position();
                children.push((name, value_off));
                Decoder::skip_value(&mut cursor)?;
            }
            Ok(children)
        }
        TypeTag::List => {
            let mut len_buf = [0u8; 4];
            cursor.read_exact(&mut len_buf)?;
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut children = Vec::with_capacity(len);
            for i in 0..len {
                let item_off = offset + cursor.position();
                children.push((format!("[{}]", i), item_off));
                Decoder::skip_value(&mut cursor)?;
            }
            Ok(children)
        }
        TypeTag::Map => {
            let mut len_buf = [0u8; 4];
            cursor.read_exact(&mut len_buf)?;
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut children = Vec::with_capacity(len);
            for i in 0..len {
                let key_val = decoder.decode_value(&mut cursor)?;
                let name = match key_val {
                    Value::String(s) => s,
                    _ => format!("[{}]", i),
                };
                let value_off = offset + cursor.position();
                children.push((name, value_off));
                Decoder::skip_value(&mut cursor)?;
            }
            Ok(children)
        }
        _ => Err(BCSError::Decoding(
            "enumerate_children_at expects struct/list/map".into(),
        )),
    }
}

fn is_protected_string_value(value: &Value) -> bool {
    matches!(
        value,
        Value::String(s) if s.starts_with(PREFIX_PBKDF2) || s.starts_with(PREFIX_KMS)
    )
}

fn is_secret_ref_string_value(value: &Value) -> bool {
    matches!(value, Value::String(s) if s.starts_with(SECRET_REF_PREFIX))
}

fn type_name_of(value: &Value) -> String {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Int8(_) => "int8",
        Value::Int16(_) => "int16",
        Value::Int32(_) => "int32",
        Value::Int64(_) => "int64",
        Value::UInt8(_) => "uint8",
        Value::UInt16(_) => "uint16",
        Value::UInt32(_) => "uint32",
        Value::UInt64(_) => "uint64",
        Value::Float32(_) => "float32",
        Value::Float64(_) => "float64",
        Value::String(_) => "string",
        Value::Bytes(_) => "bytes",
        Value::List(_) => "list",
        Value::Map(_) => "map",
        Value::Struct(_) => "struct",
        Value::Union(_, _) => "union",
        Value::Optional(_) => "optional",
    }
    .into()
}

fn type_name_of_tag(tag: TypeTag) -> &'static str {
    match tag {
        TypeTag::Null => "null",
        TypeTag::BoolFalse | TypeTag::BoolTrue => "bool",
        TypeTag::Int8 => "int8",
        TypeTag::Int16 => "int16",
        TypeTag::Int32 => "int32",
        TypeTag::Int64 => "int64",
        TypeTag::UInt8 => "uint8",
        TypeTag::UInt16 => "uint16",
        TypeTag::UInt32 => "uint32",
        TypeTag::UInt64 => "uint64",
        TypeTag::Float32 => "float32",
        TypeTag::Float64 => "float64",
        TypeTag::StringInline | TypeTag::StringExternal | TypeTag::StringInterned => "string",
        TypeTag::BytesInline | TypeTag::BytesExternal => "bytes",
        TypeTag::List => "list",
        TypeTag::Map => "map",
        TypeTag::Struct => "struct",
        TypeTag::Union => "union",
        TypeTag::OptionalSome | TypeTag::OptionalNone => "optional",
    }
}

fn leaf_preview(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(format!("\"{}\"", truncate(s, 64))),
        Value::Bool(b) => Some(b.to_string()),
        Value::Null => Some("null".into()),
        Value::Int32(v) => Some(v.to_string()),
        Value::Int64(v) => Some(v.to_string()),
        Value::UInt32(v) => Some(v.to_string()),
        Value::UInt64(v) => Some(v.to_string()),
        Value::Float64(v) => Some(v.to_string()),
        Value::List(items) => Some(format!("[{} items]", items.len())),
        Value::Struct(fields) => Some(format!("{{{} fields}}", fields.len())),
        Value::Map(entries) => Some(format!("{{{} entries}}", entries.len())),
        _ => None,
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max).collect::<String>())
    }
}

/// Peek the type tag at the start of a data-layer slice (diagnostics).
pub fn peek_type_tag(data: &[u8], offset: usize) -> Result<TypeTag> {
    if offset >= data.len() {
        return Err(BCSError::Decoding("inspect offset out of range".into()));
    }
    let mut cursor = Cursor::new(&data[offset..]);
    let mut tag = [0u8; 1];
    cursor.read_exact(&mut tag)?;
    TypeTag::from_u8(tag[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoder::Encoder;

    #[test]
    fn cursor_tree_masks_protected_marker_string() {
        let json = r#"{"host":"db","database":{"password":"__bcs_sensitive_pbkdf2__:deadbeef"}}"#;
        let bcs = Encoder::new().encode_from_json(json).unwrap();
        let mut decoder = Decoder::from_bytes(&bcs).unwrap();
        let root = InspectNode::from_decoder(&mut decoder).unwrap();
        assert_eq!(root.type_name, "struct");
        assert!(root.child_count() >= 2);

        let tree = root.format_tree().unwrap();
        assert!(tree.contains("[PROTECTED]"), "{tree}");
        assert!(!tree.contains("deadbeef"), "{tree}");
    }

    #[test]
    fn cursor_lists_top_level_field_names() {
        let json = r#"{"a":1,"b":{"c":"x"}}"#;
        let bcs = Encoder::new().encode_from_json(json).unwrap();
        let mut decoder = Decoder::from_bytes(&bcs).unwrap();
        let cursor_root = InspectNode::from_decoder(&mut decoder).unwrap();
        let names: Vec<_> = cursor_root
            .children()
            .unwrap()
            .into_iter()
            .map(|n| n.path.rsplit('.').next().unwrap_or(&n.path).to_string())
            .collect();
        assert!(names.iter().any(|n| n == "a"));
        assert!(names.iter().any(|n| n == "b"));
    }

    #[test]
    fn value_backend_still_works() {
        let v = Value::Struct(vec![
            ("x".into(), 0, Value::Int32(1)),
            ("y".into(), 0, Value::String("hi".into())),
        ]);
        let node = InspectNode::from_value("", &v, None);
        assert_eq!(node.child_count(), 2);
        let tree = node.format_tree().unwrap();
        assert!(tree.contains("x"));
        assert!(tree.contains("hi"));
    }
}

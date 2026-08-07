//! Opt-in structural string/key deduplication (header flag `0x0008`).

use crate::error::{BCSError, Result};
use crate::types::Value;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;

/// What to intern when `--dedup` is enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DedupMode {
    #[default]
    Off,
    /// Struct field names and map string keys.
    Keys,
    /// String leaf values.
    Strings,
    /// Keys and string values.
    All,
}

impl DedupMode {
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" | "none" | "" => Ok(Self::Off),
            "keys" => Ok(Self::Keys),
            "strings" => Ok(Self::Strings),
            "all" => Ok(Self::All),
            other => Err(BCSError::Encoding(format!(
                "Invalid dedup mode '{}'. Use keys, strings, or all",
                other
            ))),
        }
    }

    pub fn is_enabled(self) -> bool {
        !matches!(self, Self::Off)
    }

    fn include_keys(self) -> bool {
        matches!(self, Self::Keys | Self::All)
    }

    fn include_strings(self) -> bool {
        matches!(self, Self::Strings | Self::All)
    }
}

/// Thresholds for selecting strings into the table.
#[derive(Debug, Clone, Copy)]
pub struct DedupThresholds {
    pub min_repeats: usize,
    pub min_length: usize,
}

impl Default for DedupThresholds {
    fn default() -> Self {
        Self {
            min_repeats: 2,
            min_length: 4,
        }
    }
}

/// Sorted string dictionary written between index and data when STRUCTURAL_DEDUP is set.
#[derive(Debug, Clone, Default)]
pub struct StringTable {
    strings: Vec<String>,
    index: HashMap<String, u32>,
}

impl StringTable {
    pub fn from_value(value: &Value, mode: DedupMode, thresholds: DedupThresholds) -> Self {
        if !mode.is_enabled() {
            return Self::default();
        }
        let mut counts: HashMap<String, usize> = HashMap::new();
        collect_counts(value, mode, &mut counts, 0);
        let mut candidates: Vec<String> = counts
            .into_iter()
            .filter(|(s, n)| *n >= thresholds.min_repeats && s.len() >= thresholds.min_length)
            .map(|(s, _)| s)
            .collect();
        candidates.sort();
        let mut table = Self::default();
        for (i, s) in candidates.into_iter().enumerate() {
            table.index.insert(s.clone(), i as u32);
            table.strings.push(s);
        }
        table
    }

    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }

    pub fn len(&self) -> usize {
        self.strings.len()
    }

    pub fn lookup_id(&self, s: &str) -> Option<u32> {
        self.index.get(s).copied()
    }

    pub fn get(&self, id: u32) -> Result<&str> {
        self.strings
            .get(id as usize)
            .map(|s| s.as_str())
            .ok_or_else(|| BCSError::Decoding(format!("Interned string id {} out of range", id)))
    }

    pub fn as_arc(self) -> Arc<Self> {
        Arc::new(self)
    }

    /// Wire format: `u32 count` + (`u32 len` + utf8)*count
    pub fn write<W: Write>(&self, writer: &mut W) -> Result<()> {
        writer.write_all(&(self.strings.len() as u32).to_le_bytes())?;
        for s in &self.strings {
            let bytes = s.as_bytes();
            writer.write_all(&(bytes.len() as u32).to_le_bytes())?;
            writer.write_all(bytes)?;
        }
        Ok(())
    }

    pub fn read<R: Read>(reader: &mut R) -> Result<Self> {
        let mut count_buf = [0u8; 4];
        reader.read_exact(&mut count_buf)?;
        let count = u32::from_le_bytes(count_buf) as usize;
        if count > 1_000_000 {
            return Err(BCSError::Decoding(format!(
                "String table count {} exceeds limit",
                count
            )));
        }
        let mut table = Self::default();
        for i in 0..count {
            let mut len_buf = [0u8; 4];
            reader.read_exact(&mut len_buf)?;
            let len = u32::from_le_bytes(len_buf) as usize;
            if len > crate::limits::MAX_STRING_LEN {
                return Err(BCSError::Decoding(format!(
                    "String table entry {} length {} exceeds limit",
                    i, len
                )));
            }
            let mut buf = vec![0u8; len];
            reader.read_exact(&mut buf)?;
            let s = String::from_utf8(buf)
                .map_err(|e| BCSError::Decoding(format!("Invalid UTF-8 in string table: {}", e)))?;
            table.index.insert(s.clone(), i as u32);
            table.strings.push(s);
        }
        Ok(table)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.write(&mut buf)?;
        Ok(buf)
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        use std::io::Cursor;
        let mut cursor = Cursor::new(data);
        let table = Self::read(&mut cursor)?;
        if cursor.position() as usize != data.len() {
            return Err(BCSError::Decoding(
                "Trailing bytes in string table section".into(),
            ));
        }
        Ok(table)
    }
}

fn collect_counts(
    value: &Value,
    mode: DedupMode,
    counts: &mut HashMap<String, usize>,
    depth: usize,
) {
    if depth > crate::limits::MAX_NESTING_DEPTH {
        return;
    }
    match value {
        Value::String(s) if mode.include_strings() => {
            *counts.entry(s.clone()).or_insert(0) += 1;
        }
        Value::Struct(fields) => {
            for (name, _, child) in fields {
                if mode.include_keys() {
                    *counts.entry(name.clone()).or_insert(0) += 1;
                }
                collect_counts(child, mode, counts, depth + 1);
            }
        }
        Value::Map(entries) => {
            for (key, child) in entries {
                if mode.include_keys() {
                    if let Value::String(k) = key {
                        *counts.entry(k.clone()).or_insert(0) += 1;
                    }
                }
                collect_counts(child, mode, counts, depth + 1);
            }
        }
        Value::List(items) => {
            for item in items {
                collect_counts(item, mode, counts, depth + 1);
            }
        }
        Value::Optional(Some(inner)) => collect_counts(inner, mode, counts, depth + 1),
        Value::Union(_, inner) => collect_counts(inner, mode, counts, depth + 1),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_table_for_repeated_strings() {
        let value = Value::Struct(vec![
            ("a".into(), 0, Value::String("hello-world".into())),
            ("b".into(), 0, Value::String("hello-world".into())),
            ("c".into(), 0, Value::String("unique".into())),
        ]);
        let table = StringTable::from_value(&value, DedupMode::Strings, DedupThresholds::default());
        assert!(table.lookup_id("hello-world").is_some());
        assert!(table.lookup_id("unique").is_none());
    }

    #[test]
    fn round_trip_bytes() {
        let table = StringTable {
            strings: vec!["one".into(), "two".into()],
            index: [("one".into(), 0), ("two".into(), 1)].into_iter().collect(),
        };
        let bytes = table.to_bytes().unwrap();
        let decoded = StringTable::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.get(0).unwrap(), "one");
        assert_eq!(decoded.get(1).unwrap(), "two");
    }
}

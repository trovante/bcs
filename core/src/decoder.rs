// Decoder implementation for BCS format

use crate::error::{BCSError, Result};
use crate::index::IndexTable;
use crate::limits::{self, decompress_lz4_limited};
use crate::schema::Schema;
use crate::types::{CompositeDecoder, Header, Value, HEADER_SIZE};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;
use std::sync::Arc;

const PATH_CACHE_MAX_ENTRIES: usize = 1024;

#[derive(Clone)]
struct ParsedPathCacheEntry {
    segments: Vec<crate::index::PathSegment>,
    last_access: u64,
}

/// Backing storage for decoder file bytes (owned buffer or memory map).
enum FileBacking {
    Owned(Arc<Vec<u8>>),
    Mapped(Arc<memmap2::Mmap>),
}

impl FileBacking {
    fn as_slice(&self) -> &[u8] {
        match self {
            FileBacking::Owned(data) => data.as_slice(),
            FileBacking::Mapped(mmap) => mmap.as_ref(),
        }
    }

    fn len(&self) -> usize {
        self.as_slice().len()
    }
}

impl std::ops::Index<std::ops::Range<usize>> for FileBacking {
    type Output = [u8];

    fn index(&self, index: std::ops::Range<usize>) -> &Self::Output {
        &self.as_slice()[index]
    }
}

const CHECKSUM_OFFSET_START: usize = 56;
const CHECKSUM_OFFSET_END: usize = 64;

/// Decoder for reading BCS binary format
pub struct Decoder {
    /// Parsed file header
    header: Header,

    /// Parsed schema from semantic layer
    schema: Option<Schema>,

    /// Index table for O(1) lookups
    index_table: Option<IndexTable>,

    /// Complete file data in memory (owned or memory-mapped)
    file_data: FileBacking,

    /// Cached logical (decompressed) data layer for path queries when `DATA_COMPRESSION` is set.
    decompressed_data: Option<Arc<[u8]>>,

    /// String table when `STRUCTURAL_DEDUP` is set.
    string_table: Option<Arc<crate::string_table::StringTable>>,

    /// Parsed path cache for repeated query lookups
    parsed_path_cache: HashMap<String, ParsedPathCacheEntry>,

    /// Monotonic access clock used by parsed path cache LRU policy
    parsed_path_cache_clock: u64,

    /// Whether this decoder uses memory-mapped access
    is_mmap: bool,
}

/// How a path query was resolved (for CLI metrics / diagnostics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathAccessKind {
    /// Top-level index hit; nested segments walked in-memory / via offsets.
    Indexed,
    /// No usable index; would require full decode (not used by current get path).
    Full,
    /// Nested walk after index hit (alias of Indexed for reporting depth).
    Walk,
}

impl Decoder {
    /// Create a new decoder from a file path
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = File::open(path)?;
        let mut file_data = Vec::new();
        file.read_to_end(&mut file_data)?;

        Self::from_bytes(&file_data)
    }

    /// Create a new decoder from a file path using memory-mapped I/O.
    /// Provides zero-copy access for better performance with large files.
    pub fn from_file_mmap<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path)?;
        // Safety: the file is opened read-only and the mapping is immutable.
        let mmap = unsafe { memmap2::MmapOptions::new().map(&file) }.map_err(BCSError::Io)?;

        if mmap.len() < HEADER_SIZE {
            return Err(BCSError::Format(
                "File too small to contain header".to_string(),
            ));
        }

        let mut cursor = Cursor::new(mmap.as_ref());
        let header = Header::read(&mut cursor)?;
        header.validate()?;
        Self::validate_checksum(mmap.as_ref(), header.checksum)?;

        Ok(Self {
            header,
            schema: None,
            index_table: None,
            file_data: FileBacking::Mapped(Arc::new(mmap)),
            decompressed_data: None,
            string_table: None,
            parsed_path_cache: HashMap::new(),
            parsed_path_cache_clock: 0,
            is_mmap: true,
        })
    }

    /// Create a new decoder from a byte buffer
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < HEADER_SIZE {
            return Err(BCSError::Format(
                "File too small to contain header".to_string(),
            ));
        }

        // Parse header
        let mut cursor = Cursor::new(data);
        let header = Header::read(&mut cursor)?;

        // Validate header
        header.validate()?;

        // Validate checksum
        Self::validate_checksum(data, header.checksum)?;

        Ok(Self {
            header,
            schema: None,
            index_table: None,
            file_data: FileBacking::Owned(Arc::new(data.to_vec())),
            decompressed_data: None,
            string_table: None,
            parsed_path_cache: HashMap::new(),
            parsed_path_cache_clock: 0,
            is_mmap: false,
        })
    }

    /// Check if this decoder is using memory-mapped access
    pub fn is_mmap(&self) -> bool {
        self.is_mmap
    }

    /// Logical (possibly decompressed) data layer bytes for inspect/path tooling.
    pub fn logical_data_layer(&mut self) -> Result<Arc<[u8]>> {
        self.ensure_logical_data_layer()
    }

    /// String table when `STRUCTURAL_DEDUP` is set (loads on demand).
    pub fn string_table(&mut self) -> Result<Option<Arc<crate::string_table::StringTable>>> {
        self.ensure_string_table()?;
        Ok(self.string_table.clone())
    }

    /// Top-level index entries `(field_name, data_offset)` sorted by offset.
    /// Empty when there is no index or the table has no named buckets.
    pub fn top_level_index_entries(&mut self) -> Result<Vec<(String, u64)>> {
        if self.index_table.is_none() {
            self.load_index_table()?;
        }
        if self
            .index_table
            .as_ref()
            .map(|t| t.is_empty())
            .unwrap_or(true)
        {
            return Ok(Vec::new());
        }
        self.get_index_entries()
    }

    fn get_index_entries(&self) -> Result<Vec<(String, u64)>> {
        let index_table = self.index_table.as_ref().unwrap();
        let mut entries = Vec::new();

        // Now we can extract field names directly from the index table buckets
        for bucket in &index_table.buckets {
            if !bucket.is_empty() {
                if let Some(ref field_name) = bucket.field_name {
                    entries.push((field_name.clone(), bucket.offset));
                } else {
                    // Fallback to generating a name if none stored
                    entries.push((format!("field_{}", bucket.offset), bucket.offset));
                }
            }
        }

        // Sort by offset to maintain consistent ordering
        entries.sort_by_key(|(_, offset)| *offset);

        Ok(entries)
    }

    /// Validate the file checksum
    fn validate_checksum(data: &[u8], expected_checksum: u64) -> Result<()> {
        if data.len() < HEADER_SIZE {
            return Err(BCSError::Format(
                "File too small for checksum validation".to_string(),
            ));
        }

        // Calculate checksum over everything except the checksum field itself
        let mut data_to_check = Vec::new();
        data_to_check.extend_from_slice(&data[0..CHECKSUM_OFFSET_START]); // Header before checksum
        data_to_check.extend_from_slice(&data[CHECKSUM_OFFSET_END..]); // Everything after checksum

        let calculated_checksum = crate::encoder::Encoder::calculate_crc64(&data_to_check);

        if calculated_checksum != expected_checksum {
            return Err(BCSError::Format(format!(
                "Checksum mismatch: expected 0x{:016X}, got 0x{:016X}",
                expected_checksum, calculated_checksum
            )));
        }

        Ok(())
    }

    /// Get the file header
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// Get the schema (loads if not already loaded)
    pub fn schema(&mut self) -> Result<&Schema> {
        if self.schema.is_none() {
            self.load_schema()?;
        }
        Ok(self.schema.as_ref().unwrap())
    }

    /// Get the index table (loads if not already loaded)
    pub fn index_table(&mut self) -> Result<&IndexTable> {
        if self.index_table.is_none() {
            self.load_index_table()?;
        }
        Ok(self.index_table.as_ref().unwrap())
    }

    /// Get file metadata
    pub fn metadata(&self) -> FileMetadata {
        FileMetadata {
            version_major: self.header.version_major,
            version_minor: self.header.version_minor,
            compressed: self.header.flags.compressed,
            ai_metadata: self.header.flags.ai_metadata,
            semantic_size: self.header.semantic_size,
            index_size: self.header.index_size,
            data_size: self.header.data_size,
            total_size: self.file_data.len() as u64,
        }
    }

    /// Decode the entire data layer to a Value
    /// This reconstructs the full structure by reading all indexed values
    pub fn decode_to_value(&mut self) -> Result<Value> {
        // Load index table if not already loaded
        if self.index_table.is_none() {
            self.load_index_table()?;
        }

        let data_layer = self.ensure_logical_data_layer()?;
        let data_layer = data_layer.as_ref();

        // If the data layer is empty, return null
        if data_layer.is_empty() {
            return Ok(Value::Null);
        }

        // Reconstruct the struct from all indexed values
        let index_table = self.index_table.as_ref().unwrap();

        if index_table.is_empty() {
            // No index entries - try to decode as a single value
            let mut cursor = Cursor::new(data_layer);
            let decoder = self.composite_decoder()?;
            return decoder.decode_value(&mut cursor);
        }

        // Get all entries from the index table and reconstruct the original structure
        let entries = self.get_index_entries()?;

        if entries.is_empty() {
            return Ok(Value::Null);
        }

        let mut fields = Vec::new();
        let decoder = self.composite_decoder()?;

        // Decode each indexed field using its original name and offset.
        // Invalid offsets or decode failures are integrity errors, not silent skips.
        for (field_name, offset) in entries {
            let offset = usize::try_from(offset).map_err(|_| {
                BCSError::Decoding(format!(
                    "Invalid field offset for '{}': does not fit in address space",
                    field_name
                ))
            })?;
            if offset >= data_layer.len() {
                return Err(BCSError::Decoding(format!(
                    "Invalid field offset for '{}': {} (data layer size {})",
                    field_name,
                    offset,
                    data_layer.len()
                )));
            }

            let mut cursor = Cursor::new(&data_layer[offset..]);
            let value = decoder.decode_value(&mut cursor).map_err(|e| {
                BCSError::Decoding(format!("Failed to decode field '{}': {}", field_name, e))
            })?;
            let hash = crate::index::hash_key(&field_name);
            fields.push((field_name, hash, value));
        }

        // Return as a struct with original field names.
        // Even for single-field roots we preserve object shape.
        Ok(Value::Struct(fields))
    }

    /// Decode to JSON string
    pub fn to_json(&mut self) -> Result<String> {
        let value = self.decode_to_value()?;
        let json_value = crate::convert::value_to_json(&value)?;
        serde_json::to_string_pretty(&json_value)
            .map_err(|e| BCSError::Decoding(format!("Failed to serialize to JSON: {}", e)))
    }

    /// Decode to YAML string
    pub fn to_yaml(&mut self) -> Result<String> {
        let value = self.decode_to_value()?;
        let json_value = crate::convert::value_to_json(&value)?;
        serde_yaml::to_string(&json_value)
            .map_err(|e| BCSError::Decoding(format!("Failed to serialize to YAML: {}", e)))
    }

    /// Load the semantic layer and parse schema
    fn load_schema(&mut self) -> Result<()> {
        let (semantic_offset, semantic_end) = limits::checked_section_range(
            self.file_data.len(),
            self.header.semantic_offset,
            self.header.semantic_size,
        )?;

        // Extract semantic layer data
        let semantic_data = &self.file_data[semantic_offset..semantic_end];

        // Decompress if needed
        let decompressed_data = if self.header.flags.compressed {
            decompress_lz4_limited(semantic_data)?
        } else {
            semantic_data.to_vec()
        };

        // Parse schema from MessagePack
        let schema = Schema::from_msgpack(&decompressed_data)?;

        self.schema = Some(schema);
        Ok(())
    }

    /// Load the index table from file
    fn load_index_table(&mut self) -> Result<()> {
        if self.header.index_size == 0 {
            self.index_table = Some(IndexTable::new());
            return Ok(());
        }

        let (index_offset, index_end) = limits::checked_section_range(
            self.file_data.len(),
            self.header.index_offset,
            self.header.index_size,
        )?;

        // Extract index table data
        let index_data = &self.file_data[index_offset..index_end];

        // Parse index table
        let mut cursor = Cursor::new(index_data);
        let index_table = IndexTable::read(&mut cursor)?;

        self.index_table = Some(index_table);
        Ok(())
    }

    /// Return a checked byte range for the on-disk data layer section.
    fn data_layer_range(&self) -> Result<(usize, usize)> {
        limits::checked_section_range(
            self.file_data.len(),
            self.header.data_offset,
            self.header.data_size,
        )
    }

    /// Logical data layer bytes (decompressed when `DATA_COMPRESSION` is set).
    /// Cached after first access so path queries reuse decompression work.
    fn ensure_logical_data_layer(&mut self) -> Result<Arc<[u8]>> {
        if let Some(ref cached) = self.decompressed_data {
            return Ok(cached.clone());
        }

        let (data_offset, data_end) = self.data_layer_range()?;
        let raw = &self.file_data[data_offset..data_end];
        let bytes = if self.header.flags.data_compressed {
            decompress_lz4_limited(raw)?
        } else {
            raw.to_vec()
        };
        let arc: Arc<[u8]> = Arc::from(bytes.into_boxed_slice());
        self.decompressed_data = Some(arc.clone());
        Ok(arc)
    }

    /// Load string table when STRUCTURAL_DEDUP is set (between index and data).
    fn ensure_string_table(&mut self) -> Result<()> {
        if self.string_table.is_some() || !self.header.flags.structural_dedup {
            return Ok(());
        }
        let table_start = self.header.index_offset + self.header.index_size;
        let table_end = self.header.data_offset;
        if table_end < table_start {
            return Err(BCSError::Format(
                "Invalid string table range (data_offset before end of index)".into(),
            ));
        }
        let start = usize::try_from(table_start)
            .map_err(|_| BCSError::Format("string table offset overflow".into()))?;
        let end = usize::try_from(table_end)
            .map_err(|_| BCSError::Format("string table end overflow".into()))?;
        if end > self.file_data.len() {
            return Err(BCSError::Format(
                "String table extends past end of file".into(),
            ));
        }
        let bytes = &self.file_data[start..end];
        let table = crate::string_table::StringTable::from_bytes(bytes)?;
        self.string_table = Some(Arc::new(table));
        Ok(())
    }

    fn composite_decoder(&mut self) -> Result<CompositeDecoder> {
        self.ensure_string_table()?;
        let mut decoder = CompositeDecoder::new();
        if let Some(table) = &self.string_table {
            decoder = decoder.with_string_table(table.clone());
        }
        Ok(decoder)
    }

    /// Get a value by path without building the full document value tree when an index is present.
    ///
    /// Decodes the indexed top-level field, then walks nested segments in that subtree.
    /// When the data layer is LZ4-compressed, the layer is decompressed once and cached.
    pub fn get(&mut self, path: &str) -> Result<Value> {
        if path.contains(".$.") || path.contains("[$]") {
            return self.get_wildcard(path);
        }
        self.get_with_offset(path).map(|(_, value)| value)
    }

    /// Alias for [`Self::get`] — documents the partial-read path API.
    pub fn get_path(&mut self, path: &str) -> Result<Value> {
        self.get(path)
    }

    /// Resolve path and report access kind (`indexed` for index-table hit, `walk` when nested).
    pub fn get_path_with_access(&mut self, path: &str) -> Result<(Value, PathAccessKind)> {
        if path.contains(".$.") || path.contains("[$]") {
            let value = self.get_wildcard(path)?;
            return Ok((value, PathAccessKind::Walk));
        }
        let segments = self.parse_path_cached(path)?;
        if self.index_table.is_none() {
            self.load_index_table()?;
        }
        let exact_index_hit = self
            .index_table
            .as_ref()
            .and_then(|t| t.lookup_key_exact(path))
            .is_some();
        let value = self.get_with_offset(path).map(|(_, v)| v)?;
        let kind = if exact_index_hit || segments.len() <= 1 {
            PathAccessKind::Indexed
        } else {
            PathAccessKind::Walk
        };
        Ok((value, kind))
    }

    /// Resolve wildcard paths such as services.$.routes.$.paths
    pub fn get_wildcard(&mut self, path: &str) -> Result<Value> {
        let segments = self.parse_path_cached(path)?;
        if segments.is_empty() {
            return Err(BCSError::Index("Empty path".to_string()));
        }

        let first_key = match &segments[0] {
            crate::index::PathSegment::Field(key) => key.clone(),
            _ => {
                return Err(BCSError::Index(
                    "Path must start with a field name".to_string(),
                ))
            }
        };

        let (_, top_value) = self.get_top_level_field_value(&first_key)?;
        let mut values = Vec::new();
        Self::walk_wildcard_values(&top_value, &segments[1..], &mut values)?;
        Ok(Value::List(values))
    }

    fn walk_wildcard_values(
        value: &Value,
        segments: &[crate::index::PathSegment],
        out: &mut Vec<Value>,
    ) -> Result<()> {
        if segments.is_empty() {
            out.push(value.clone());
            return Ok(());
        }

        match &segments[0] {
            crate::index::PathSegment::Field(field_name) => match value {
                Value::Struct(fields) => {
                    for (name, _hash, child) in fields {
                        if name == field_name {
                            return Self::walk_wildcard_values(child, &segments[1..], out);
                        }
                    }
                    Ok(())
                }
                Value::Map(entries) => {
                    for (key, child) in entries {
                        if let Value::String(k) = key {
                            if k == field_name {
                                return Self::walk_wildcard_values(child, &segments[1..], out);
                            }
                        }
                    }
                    Ok(())
                }
                _ => Ok(()),
            },
            crate::index::PathSegment::Index(idx) => match value {
                Value::List(items) => {
                    if *idx >= items.len() {
                        return Ok(());
                    }
                    Self::walk_wildcard_values(&items[*idx], &segments[1..], out)
                }
                _ => Ok(()),
            },
            crate::index::PathSegment::WildcardIndex => match value {
                Value::List(items) => {
                    for item in items {
                        Self::walk_wildcard_values(item, &segments[1..], out)?;
                    }
                    Ok(())
                }
                _ => Ok(()),
            },
        }
    }

    /// Resolve a path and return the physical byte offset and decoded value.
    ///
    /// The offset corresponds to the start of the value bytes in the **logical**
    /// (possibly decompressed) data layer. Does not build the full document tree.
    pub fn get_with_offset(&mut self, path: &str) -> Result<(u64, Value)> {
        let segments = self.parse_path_cached(path)?;

        if segments.is_empty() {
            return Err(BCSError::Index("Empty path".to_string()));
        }

        if self.index_table.is_none() {
            self.load_index_table()?;
        }
        self.ensure_string_table()?;

        // Prefer exact nested index hit (from --index-maps-over).
        if let Some(offset) = self
            .index_table
            .as_ref()
            .and_then(|t| t.lookup_key_exact(path))
        {
            let data_layer = self.ensure_logical_data_layer()?;
            let off = usize::try_from(offset)
                .map_err(|_| BCSError::Index("Invalid offset in index table".to_string()))?;
            if off >= data_layer.len() {
                return Err(BCSError::Index("Invalid offset in index table".to_string()));
            }
            let decoder = self.composite_decoder()?;
            let mut cursor = Cursor::new(&data_layer[off..]);
            let value = decoder.decode_value(&mut cursor)?;
            return Ok((offset, value));
        }

        let first_key = match &segments[0] {
            crate::index::PathSegment::Field(key) => key.clone(),
            _ => {
                return Err(BCSError::Index(
                    "Path must start with a field name".to_string(),
                ))
            }
        };

        let (top_offset, mut current_value) = self.get_top_level_field_value(&first_key)?;
        let mut current_offset = top_offset;

        let data_layer = self.ensure_logical_data_layer()?;
        let data_layer = data_layer.as_ref();
        let string_table = self.string_table.clone();

        for segment in &segments[1..] {
            match segment {
                crate::index::PathSegment::Field(field_name) => {
                    match current_value {
                        Value::Struct(fields) => {
                            let mut found = None;
                            for (name, _hash, field_value) in fields {
                                if name == *field_name {
                                    found = Some((name, field_value));
                                    break;
                                }
                            }

                            if let Some((_name, value)) = found {
                                // Resolve nested offset by scanning encoded struct fields.
                                // Layout: [tag][field_count:u32][field_name:string][hash:u64][value]...
                                let nested = nested_struct_field_relative_offset_ex(
                                    &data_layer[current_offset as usize..],
                                    field_name,
                                    string_table.clone(),
                                )
                                .map(|rel| current_offset + rel)?;
                                current_offset = nested;
                                current_value = value;
                            } else {
                                return Err(BCSError::Index(format!(
                                    "Field '{}' not found in struct",
                                    field_name
                                )));
                            }
                        }
                        Value::Map(entries) => {
                            let mut found = None;
                            for (key, val) in entries {
                                if let Value::String(key_str) = key {
                                    if key_str == *field_name {
                                        found = Some(val);
                                        break;
                                    }
                                }
                            }

                            if let Some(value) = found {
                                // Map doesn't carry direct child offsets in exposed model.
                                // Keep best-known physical offset (map root).
                                current_value = value;
                            } else {
                                return Err(BCSError::Index(format!(
                                    "Key '{}' not found in map",
                                    field_name
                                )));
                            }
                        }
                        _ => {
                            return Err(BCSError::Index(format!(
                                "Cannot access field '{}' on non-struct/map value",
                                field_name
                            )));
                        }
                    }
                }
                crate::index::PathSegment::Index(idx) => {
                    match current_value {
                        Value::List(items) => {
                            if *idx >= items.len() {
                                return Err(BCSError::Index(format!(
                                    "Index {} out of bounds (length: {})",
                                    idx,
                                    items.len()
                                )));
                            }

                            // Resolve nested list element physical offset when possible.
                            // Layout: [tag][len:u32][item...]
                            let nested = Self::resolve_nested_list_index_offset(
                                data_layer,
                                current_offset,
                                *idx,
                            )?;
                            current_offset = nested;
                            current_value = items.into_iter().nth(*idx).ok_or_else(|| {
                                BCSError::Index(format!(
                                    "Index {} out of bounds while consuming list",
                                    idx
                                ))
                            })?;
                        }
                        _ => {
                            return Err(BCSError::Index("Cannot index non-list value".to_string()));
                        }
                    }
                }
                crate::index::PathSegment::WildcardIndex => {
                    return Err(BCSError::Index(
                        "Wildcard path is not supported in get_with_offset; use get() instead"
                            .to_string(),
                    ));
                }
            }
        }

        Ok((current_offset, current_value))
    }

    fn get_top_level_field_value(&mut self, first_key: &str) -> Result<(u64, Value)> {
        if self.index_table.is_none() {
            self.load_index_table()?;
        }

        let top_offset = self
            .index_table
            .as_ref()
            .unwrap()
            .lookup_key_exact(first_key)
            .ok_or_else(|| BCSError::Index(format!("Key '{}' not found", first_key)))?;

        let data_layer = self.ensure_logical_data_layer()?;
        let data_layer = data_layer.as_ref();

        let top_offset_usize = usize::try_from(top_offset)
            .map_err(|_| BCSError::Index("Invalid offset in index table".to_string()))?;
        if top_offset_usize >= data_layer.len() {
            return Err(BCSError::Index("Invalid offset in index table".to_string()));
        }

        let decoder = self.composite_decoder()?;
        let mut cursor = Cursor::new(&data_layer[top_offset_usize..]);
        let value = decoder.decode_value(&mut cursor)?;

        Ok((top_offset, value))
    }

    fn parse_path_cached(&mut self, path: &str) -> Result<Vec<crate::index::PathSegment>> {
        self.parsed_path_cache_clock = self.parsed_path_cache_clock.wrapping_add(1);

        if let Some(cached) = self.parsed_path_cache.get_mut(path) {
            cached.last_access = self.parsed_path_cache_clock;
            return Ok(cached.segments.clone());
        }

        let parsed = crate::index::parse_path(path)?;

        if self.parsed_path_cache.len() >= PATH_CACHE_MAX_ENTRIES {
            let lru_key = self
                .parsed_path_cache
                .iter()
                .min_by_key(|(_, entry)| entry.last_access)
                .map(|(key, _)| key.clone());

            if let Some(key) = lru_key {
                self.parsed_path_cache.remove(&key);
            }
        }
        self.parsed_path_cache.insert(
            path.to_string(),
            ParsedPathCacheEntry {
                segments: parsed.clone(),
                last_access: self.parsed_path_cache_clock,
            },
        );

        Ok(parsed)
    }
}

/// Relative offset of a struct field's value payload within `struct_bytes` (which starts at tag `0x42`).
pub fn nested_struct_field_relative_offset(
    struct_bytes: &[u8],
    target_field_name: &str,
) -> Result<u64> {
    nested_struct_field_relative_offset_ex(struct_bytes, target_field_name, None)
}

/// Like [`nested_struct_field_relative_offset`] but resolves interned field names via `table`.
pub fn nested_struct_field_relative_offset_ex(
    struct_bytes: &[u8],
    target_field_name: &str,
    string_table: Option<std::sync::Arc<crate::string_table::StringTable>>,
) -> Result<u64> {
    let mut cursor = Cursor::new(struct_bytes);
    let mut tag = [0u8; 1];
    cursor.read_exact(&mut tag)?;

    if tag[0] != 0x42 {
        return Err(BCSError::Index(
            "Value at offset is not a struct".to_string(),
        ));
    }

    let mut count_buf = [0u8; 4];
    cursor.read_exact(&mut count_buf)?;
    let count = u32::from_le_bytes(count_buf) as usize;

    let mut decoder = CompositeDecoder::new();
    if let Some(table) = string_table {
        decoder = decoder.with_string_table(table);
    }
    for _ in 0..count {
        let field_name_value = decoder.decode_value(&mut cursor)?;
        let field_name = match field_name_value {
            Value::String(name) => name,
            _ => {
                return Err(BCSError::Decoding(
                    "Invalid struct field name encoding".to_string(),
                ));
            }
        };

        let mut hash_buf = [0u8; 8];
        cursor.read_exact(&mut hash_buf)?;

        let value_offset = cursor.position();
        if field_name == target_field_name {
            return Ok(value_offset);
        }

        Decoder::skip_value(&mut cursor)?;
    }

    Err(BCSError::Index("Struct field offset not found".to_string()))
}

impl Decoder {
    fn resolve_nested_list_index_offset(
        data_layer: &[u8],
        list_offset: u64,
        target_index: usize,
    ) -> Result<u64> {
        let start = list_offset as usize;
        if start >= data_layer.len() {
            return Err(BCSError::Index("Invalid list offset".to_string()));
        }

        let mut cursor = Cursor::new(&data_layer[start..]);
        let mut tag = [0u8; 1];
        cursor.read_exact(&mut tag)?;

        // TypeTag::List = 0x40
        if tag[0] != 0x40 {
            return Err(BCSError::Index("Value at offset is not a list".to_string()));
        }

        let mut len_buf = [0u8; 4];
        cursor.read_exact(&mut len_buf)?;
        let len = u32::from_le_bytes(len_buf) as usize;

        if target_index >= len {
            return Err(BCSError::Index(format!(
                "Index {} out of bounds (length: {})",
                target_index, len
            )));
        }

        for i in 0..len {
            let item_relative_offset = cursor.position() as usize;

            if i == target_index {
                return Ok(list_offset + item_relative_offset as u64);
            }

            let before = cursor.position();
            Self::skip_value(&mut cursor)?;
            let after = cursor.position();

            if after <= before {
                return Err(BCSError::Decoding(
                    "Failed to advance while scanning list".to_string(),
                ));
            }
        }

        Err(BCSError::Index("List index offset not found".to_string()))
    }

    fn skip_bytes(cursor: &mut Cursor<&[u8]>, mut len: usize) -> Result<()> {
        let mut scratch = [0u8; 256];
        while len > 0 {
            let chunk = len.min(scratch.len());
            cursor.read_exact(&mut scratch[..chunk])?;
            len -= chunk;
        }
        Ok(())
    }

    fn read_u32_le(cursor: &mut Cursor<&[u8]>) -> Result<u32> {
        let mut buf = [0u8; 4];
        cursor.read_exact(&mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    pub(crate) fn skip_value(cursor: &mut Cursor<&[u8]>) -> Result<()> {
        let mut tag_buf = [0u8; 1];
        cursor.read_exact(&mut tag_buf)?;
        let tag = tag_buf[0];

        match tag {
            // Null / bool
            0x00..=0x02 => Ok(()),

            // Fixed-width numeric primitives
            0x10 | 0x14 => Self::skip_bytes(cursor, 1),
            0x11 | 0x15 => Self::skip_bytes(cursor, 2),
            0x12 | 0x16 | 0x20 => Self::skip_bytes(cursor, 4),
            0x13 | 0x17 | 0x21 => Self::skip_bytes(cursor, 8),

            // String / bytes
            0x30 | 0x32 => {
                let mut len_buf = [0u8; 1];
                cursor.read_exact(&mut len_buf)?;
                Self::skip_bytes(cursor, len_buf[0] as usize)
            }
            0x31 | 0x33 => {
                let len = Self::read_u32_le(cursor)? as usize;
                Self::skip_bytes(cursor, len)
            }
            // Interned string: u32 id
            0x34 => Self::skip_bytes(cursor, 4),

            // List
            0x40 => {
                let len = Self::read_u32_le(cursor)? as usize;
                for _ in 0..len {
                    Self::skip_value(cursor)?;
                }
                Ok(())
            }

            // Map (key-value)
            0x41 => {
                let len = Self::read_u32_le(cursor)? as usize;
                for _ in 0..len {
                    Self::skip_value(cursor)?;
                    Self::skip_value(cursor)?;
                }
                Ok(())
            }

            // Struct: field-name string + hash(u64) + value
            0x42 => {
                let count = Self::read_u32_le(cursor)? as usize;

                for _ in 0..count {
                    Self::skip_value(cursor)?;
                    Self::skip_bytes(cursor, 8)?;
                    Self::skip_value(cursor)?;
                }
                Ok(())
            }

            // Union: variant tag(u32) + value
            0x43 => {
                Self::skip_bytes(cursor, 4)?;
                Self::skip_value(cursor)
            }

            // Optional
            0x44 => Self::skip_value(cursor),
            0x45 => Ok(()),

            _ => Err(BCSError::Decoding(format!(
                "Unknown type tag while skipping: 0x{:02X}",
                tag
            ))),
        }
    }

    /// Returns the current number of cached parsed paths.
    ///
    /// This is primarily intended for diagnostics and testing.
    pub fn parsed_path_cache_len(&self) -> usize {
        self.parsed_path_cache.len()
    }

    /// Returns whether a path currently exists in parsed path cache.
    pub fn parsed_path_cache_contains(&self, path: &str) -> bool {
        self.parsed_path_cache.contains_key(path)
    }

    /// Check if a path exists
    pub fn has(&mut self, path: &str) -> bool {
        self.get(path).is_ok()
    }

    /// Create a streaming iterator over key-value pairs
    pub fn stream(&mut self) -> Result<StreamingDecoder> {
        // Load index table if not already loaded
        if self.index_table.is_none() {
            self.load_index_table()?;
        }

        let data = self.ensure_logical_data_layer()?.to_vec();
        let decoder = self.composite_decoder()?;
        let entries = if self
            .index_table
            .as_ref()
            .map(|t| !t.is_empty())
            .unwrap_or(false)
        {
            Some(self.get_index_entries()?)
        } else {
            None
        };

        Ok(StreamingDecoder {
            data,
            position: 0,
            decoder,
            entries,
            entry_index: 0,
        })
    }
}

/// Streaming decoder that yields key-value pairs incrementally
pub struct StreamingDecoder {
    data: Vec<u8>,
    position: usize,
    decoder: CompositeDecoder,
    /// When present, iterate indexed top-level fields instead of a flat byte scan.
    entries: Option<Vec<(String, u64)>>,
    entry_index: usize,
}

impl StreamingDecoder {
    /// Get the next named field from an indexed stream, if available.
    pub fn next_entry(&mut self) -> Result<Option<(String, Value)>> {
        let Some(entries) = &self.entries else {
            return Ok(None);
        };
        if self.entry_index >= entries.len() {
            return Ok(None);
        }

        let (name, offset) = entries[self.entry_index].clone();
        self.entry_index += 1;

        let offset = usize::try_from(offset).map_err(|_| {
            BCSError::Decoding(format!("Invalid streaming offset for field '{}'", name))
        })?;
        if offset >= self.data.len() {
            return Err(BCSError::Decoding(format!(
                "Streaming offset out of bounds for field '{}'",
                name
            )));
        }

        let mut cursor = Cursor::new(&self.data[offset..]);
        let value = self.decoder.decode_value(&mut cursor)?;
        Ok(Some((name, value)))
    }

    /// Get the next value from the stream
    pub fn next_value(&mut self) -> Result<Option<Value>> {
        if let Some(entries) = &self.entries {
            if self.entry_index >= entries.len() {
                return Ok(None);
            }
            return self.next_entry().map(|opt| opt.map(|(_, value)| value));
        }

        if self.position >= self.data.len() {
            return Ok(None);
        }

        let mut cursor = Cursor::new(&self.data[self.position..]);
        let value = self.decoder.decode_value(&mut cursor)?;

        // Update position based on how much was read
        let bytes_read = cursor.position() as usize;
        self.position += bytes_read;

        Ok(Some(value))
    }

    /// Check if there are more values to read
    pub fn has_next(&self) -> bool {
        if let Some(entries) = &self.entries {
            return self.entry_index < entries.len();
        }
        self.position < self.data.len()
    }
}

impl Iterator for StreamingDecoder {
    type Item = Result<Value>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next_value() {
            Ok(Some(value)) => Some(Ok(value)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

/// File metadata information
#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub version_major: u8,
    pub version_minor: u8,
    pub compressed: bool,
    /// Reserved header bit `0x0002` as observed on disk (ignored for semantics).
    pub ai_metadata: bool,
    pub semantic_size: u64,
    pub index_size: u64,
    pub data_size: u64,
    pub total_size: u64,
}

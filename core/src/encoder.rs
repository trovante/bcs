// Encoder implementation for BCS format

use crate::error::{BCSError, Result};
use crate::index::{IndexTable, IndexTableBuilder};
use crate::schema::Schema;
use crate::types::{CompositeEncoder, Header, Value, HEADER_SIZE};
use std::collections::HashMap;

const CHECKSUM_OFFSET_START: usize = 56;
const CHECKSUM_OFFSET_END: usize = 64;

/// Configuration options for the encoder
#[derive(Debug, Clone)]
pub struct EncoderConfig {
    /// Enable LZ4 compression for semantic layer
    pub compression: bool,

    /// Reserved; ignored. Header bit `0x0002` is never written by current encoders.
    ///
    /// Historical name: early drafts called this "AI metadata" but no embeddings
    /// or tags were ever stored. Kept so existing `EncoderConfig { .. }` literals compile.
    pub ai_metadata: bool,

    /// Include semantic layer (embedded schema)
    pub include_semantic_layer: bool,

    /// Include index table for O(1) path lookup
    pub include_index_table: bool,

    /// Compress data layer with LZ4
    pub data_compression: bool,

    /// Opt-in structural string/key dedup (`STRUCTURAL_DEDUP`)
    pub dedup: crate::string_table::DedupMode,

    /// Thresholds for selecting interned strings
    pub dedup_thresholds: crate::string_table::DedupThresholds,

    /// When set, also index nested fields of structs/maps with at least this many entries.
    pub index_maps_over: Option<usize>,
}

impl Default for EncoderConfig {
    fn default() -> Self {
        Self {
            compression: true,
            ai_metadata: false,
            include_semantic_layer: true,
            include_index_table: true,
            data_compression: false,
            dedup: crate::string_table::DedupMode::Off,
            dedup_thresholds: crate::string_table::DedupThresholds::default(),
            index_maps_over: None,
        }
    }
}

impl EncoderConfig {
    /// Derive encoder settings from an existing BCS header so rewrite flows
    /// (protect/reindex) can preserve the source file profile.
    pub fn from_header(header: &crate::types::Header) -> Self {
        Self {
            compression: header.flags.compressed && header.semantic_size > 0,
            // Reserved bit is never preserved on rewrite.
            ai_metadata: false,
            include_semantic_layer: header.semantic_size > 0,
            include_index_table: header.index_size > 0,
            data_compression: header.flags.data_compressed,
            // Dedup table is rebuilt from values when rewriting; default off unless caller sets it.
            dedup: if header.flags.structural_dedup {
                crate::string_table::DedupMode::All
            } else {
                crate::string_table::DedupMode::Off
            },
            dedup_thresholds: crate::string_table::DedupThresholds::default(),
            index_maps_over: None,
        }
    }
}

/// Encoder for converting structured data to BCS binary format
pub struct Encoder {
    /// Configuration options
    config: EncoderConfig,

    /// Buffer for building the semantic layer
    semantic_buffer: Vec<u8>,

    /// Buffer for building the binary data layer
    data_buffer: Vec<u8>,

    /// Index table builder for tracking offsets
    index_builder: IndexTableBuilder,

    /// Current offset in the data layer
    current_offset: u64,

    /// Schema to use for validation (optional)
    schema: Option<Schema>,

    /// Active string table for the current encode (set during encode_value_with_schema)
    string_table: Option<std::sync::Arc<crate::string_table::StringTable>>,
}

impl Encoder {
    /// Create a new encoder with default configuration
    pub fn new() -> Self {
        Self {
            config: EncoderConfig::default(),
            semantic_buffer: Vec::new(),
            data_buffer: Vec::new(),
            index_builder: IndexTableBuilder::new(),
            current_offset: 0,
            schema: None,
            string_table: None,
        }
    }

    /// Create a new encoder with custom configuration
    pub fn with_config(config: EncoderConfig) -> Self {
        Self {
            config,
            semantic_buffer: Vec::new(),
            data_buffer: Vec::new(),
            index_builder: IndexTableBuilder::new(),
            current_offset: 0,
            schema: None,
            string_table: None,
        }
    }

    /// Set the schema for validation
    pub fn with_schema(mut self, schema: Schema) -> Self {
        self.schema = Some(schema);
        self
    }

    /// Enable or disable compression
    pub fn set_compression(&mut self, enabled: bool) {
        self.config.compression = enabled;
    }

    /// No-op: header bit `0x0002` is reserved and is never written.
    #[deprecated(
        since = "0.1.0",
        note = "header bit 0x0002 is reserved; writers always clear it"
    )]
    pub fn set_ai_metadata(&mut self, _enabled: bool) {
        self.config.ai_metadata = false;
    }

    /// Enable or disable semantic layer embedding.
    pub fn set_include_semantic_layer(&mut self, enabled: bool) {
        self.config.include_semantic_layer = enabled;
    }

    /// Enable or disable index table embedding.
    pub fn set_include_index_table(&mut self, enabled: bool) {
        self.config.include_index_table = enabled;
    }

    /// Compact mode favors size by omitting semantic layer and index table.
    pub fn set_compact_mode(&mut self, enabled: bool) {
        if enabled {
            self.config.include_semantic_layer = false;
            self.config.include_index_table = false;
            self.config.compression = false;
            self.config.ai_metadata = false;
        }
    }

    /// Enable or disable data layer compression.
    pub fn set_data_compression(&mut self, enabled: bool) {
        self.config.data_compression = enabled;
    }

    /// Set structural dedup mode (`keys` / `strings` / `all`).
    pub fn set_dedup(&mut self, mode: crate::string_table::DedupMode) {
        self.config.dedup = mode;
    }

    /// Set dedup thresholds (min repeats / min length).
    pub fn set_dedup_thresholds(&mut self, thresholds: crate::string_table::DedupThresholds) {
        self.config.dedup_thresholds = thresholds;
    }

    /// Index nested fields of large maps/structs (threshold entry count).
    pub fn set_index_maps_over(&mut self, threshold: Option<usize>) {
        self.config.index_maps_over = threshold;
    }

    /// Get the current configuration
    pub fn config(&self) -> &EncoderConfig {
        &self.config
    }

    /// Get the current offset in the data layer
    pub fn current_offset(&self) -> u64 {
        self.current_offset
    }

    /// Track an offset for a top-level key
    pub fn track_offset(&mut self, key: String, offset: u64) {
        self.index_builder.add_entry(key, offset);
    }

    /// Write a value to the data buffer and return its offset
    pub fn write_value(&mut self, value: &Value) -> Result<u64> {
        let offset = self.current_offset;

        // Encode the value to a temporary buffer
        let mut temp_buffer = Vec::new();
        let mut encoder = CompositeEncoder::new();
        if let Some(table) = &self.string_table {
            encoder = encoder.with_string_table(table.clone());
        }
        encoder.encode_value(&mut temp_buffer, value)?;

        // Append to data buffer
        self.data_buffer.extend_from_slice(&temp_buffer);

        // Update current offset
        self.current_offset += temp_buffer.len() as u64;

        Ok(offset)
    }

    /// Build the semantic layer (schema encoded as MessagePack)
    fn build_semantic_layer(&mut self, schema: &Schema) -> Result<Vec<u8>> {
        // Encode schema to MessagePack
        let msgpack_data = schema.to_msgpack()?;

        // Compress if enabled
        if self.config.compression {
            // Use LZ4 compression with size prepended
            let compressed = lz4::block::compress(&msgpack_data, None, true)
                .map_err(|e| BCSError::Encoding(format!("LZ4 compression failed: {}", e)))?;
            Ok(compressed)
        } else {
            Ok(msgpack_data)
        }
    }

    /// Build the index table
    fn build_index_table(&mut self) -> Result<IndexTable> {
        // Take ownership of the builder and build the table
        let builder = std::mem::take(&mut self.index_builder);
        builder.build()
    }

    /// Reset the encoder state for a new encoding operation
    pub fn reset(&mut self) {
        self.semantic_buffer.clear();
        self.data_buffer.clear();
        self.index_builder = IndexTableBuilder::new();
        self.current_offset = 0;
        self.string_table = None;
    }

    /// Get the size of the data buffer
    pub fn data_size(&self) -> usize {
        self.data_buffer.len()
    }

    /// Get the size of the semantic buffer
    pub fn semantic_size(&self) -> usize {
        self.semantic_buffer.len()
    }
}

impl Default for Encoder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// JSON Encoding
// ============================================================================

impl Encoder {
    /// Encode from JSON string
    pub fn encode_from_json(&mut self, json: &str) -> Result<Vec<u8>> {
        // Parse JSON
        let json_value: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| BCSError::Encoding(format!("Failed to parse JSON: {}", e)))?;

        // Convert to BCS Value
        let value = Self::json_to_value(&json_value)?;

        // Infer or use provided schema
        let schema = if let Some(ref s) = self.schema {
            s.clone()
        } else {
            Self::infer_schema(&value)?
        };

        // Validate if schema is provided
        if self.schema.is_some() {
            let engine = crate::schema::SchemaEngine::new();
            let validation_result = engine.validate(&value, &schema);
            if !validation_result.is_valid() {
                let errors: Vec<String> = validation_result
                    .errors
                    .iter()
                    .map(|e| format!("{}: {}", e.path, e.message))
                    .collect();
                return Err(BCSError::Validation(format!(
                    "Validation failed:\n{}",
                    errors.join("\n")
                )));
            }
        }

        // Encode the data
        self.encode_value_with_schema(&value, &schema)
    }

    /// Convert serde_json::Value to BCS Value
    fn json_to_value(json: &serde_json::Value) -> Result<Value> {
        match json {
            serde_json::Value::Null => Ok(Value::Null),
            serde_json::Value::Bool(b) => Ok(Value::Bool(*b)),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    // Choose appropriate integer type based on value
                    if i >= i32::MIN as i64 && i <= i32::MAX as i64 {
                        Ok(Value::Int32(i as i32))
                    } else {
                        Ok(Value::Int64(i))
                    }
                } else if let Some(u) = n.as_u64() {
                    if u <= u32::MAX as u64 {
                        Ok(Value::UInt32(u as u32))
                    } else {
                        Ok(Value::UInt64(u))
                    }
                } else if let Some(f) = n.as_f64() {
                    Ok(Value::Float64(f))
                } else {
                    Err(BCSError::Encoding("Invalid number in JSON".to_string()))
                }
            }
            serde_json::Value::String(s) => Ok(Value::String(s.clone())),
            serde_json::Value::Array(arr) => {
                let values: Result<Vec<Value>> = arr.iter().map(Self::json_to_value).collect();
                Ok(Value::List(values?))
            }
            serde_json::Value::Object(obj) => {
                // Convert to struct with original field names and hashes
                let mut fields = Vec::new();
                for (key, val) in obj {
                    let hash = crate::index::hash_key(key);
                    let value = Self::json_to_value(val)?;
                    fields.push((key.clone(), hash, value));
                }
                Ok(Value::Struct(fields))
            }
        }
    }

    /// Infer a schema from a value
    fn infer_schema(value: &Value) -> Result<Schema> {
        let mut schema = Schema::new("Root".to_string());

        // Infer the root type
        let root_type = Self::infer_type(value);
        schema.add_type("Root".to_string(), root_type);

        Ok(schema)
    }

    /// Infer a type definition from a value
    fn infer_type(value: &Value) -> crate::schema::TypeDefinition {
        use crate::schema::{FieldDefinition, TypeDefinition};

        match value {
            Value::Null => TypeDefinition::Null,
            Value::Bool(_) => TypeDefinition::Bool,
            Value::Int8(_) => TypeDefinition::Int8,
            Value::Int16(_) => TypeDefinition::Int16,
            Value::Int32(_) => TypeDefinition::Int32,
            Value::Int64(_) => TypeDefinition::Int64,
            Value::UInt8(_) => TypeDefinition::UInt8,
            Value::UInt16(_) => TypeDefinition::UInt16,
            Value::UInt32(_) => TypeDefinition::UInt32,
            Value::UInt64(_) => TypeDefinition::UInt64,
            Value::Float32(_) => TypeDefinition::Float32,
            Value::Float64(_) => TypeDefinition::Float64,
            Value::String(_) => TypeDefinition::String,
            Value::Bytes(_) => TypeDefinition::Bytes,
            Value::List(items) => {
                if items.is_empty() {
                    TypeDefinition::List(Box::new(TypeDefinition::String))
                } else {
                    let mut unified = Self::infer_type(&items[0]);
                    for item in items.iter().skip(1) {
                        unified = Self::unify_types(unified, Self::infer_type(item));
                    }
                    TypeDefinition::List(Box::new(unified))
                }
            }
            Value::Map(entries) => {
                if entries.is_empty() {
                    TypeDefinition::Map(
                        Box::new(TypeDefinition::String),
                        Box::new(TypeDefinition::String),
                    )
                } else {
                    let mut key_type = Self::infer_type(&entries[0].0);
                    let mut val_type = Self::infer_type(&entries[0].1);
                    for (key, val) in entries.iter().skip(1) {
                        key_type = Self::unify_types(key_type, Self::infer_type(key));
                        val_type = Self::unify_types(val_type, Self::infer_type(val));
                    }
                    TypeDefinition::Map(Box::new(key_type), Box::new(val_type))
                }
            }
            Value::Struct(fields) => {
                let mut sorted_fields = fields.clone();
                sorted_fields.sort_by(|a, b| a.0.cmp(&b.0));

                let mut field_defs = HashMap::new();
                for (field_name, _hash, val) in &sorted_fields {
                    // Use the original field name
                    let field_type = Self::infer_type(val);
                    field_defs.insert(
                        field_name.clone(),
                        FieldDefinition {
                            field_type,
                            required: true,
                            default: None,
                            constraints: Vec::new(),
                            documentation: None,
                            ai_tag: None,
                        },
                    );
                }
                TypeDefinition::Struct(field_defs)
            }
            Value::Union(_, val) => {
                // For unions, we'd need more context
                Self::infer_type(val)
            }
            Value::Optional(opt) => {
                if let Some(val) = opt {
                    let inner_type = Self::infer_type(val);
                    TypeDefinition::Optional(Box::new(inner_type))
                } else {
                    TypeDefinition::Optional(Box::new(TypeDefinition::Null))
                }
            }
        }
    }

    /// Unify two inferred types, promoting compatible numerics and falling back to string.
    fn unify_types(
        left: crate::schema::TypeDefinition,
        right: crate::schema::TypeDefinition,
    ) -> crate::schema::TypeDefinition {
        use crate::schema::TypeDefinition;

        if left == right {
            return left;
        }

        let left_num = Self::is_numeric_type(&left);
        let right_num = Self::is_numeric_type(&right);
        if left_num && right_num {
            if matches!(left, TypeDefinition::Float64)
                || matches!(right, TypeDefinition::Float64)
                || matches!(left, TypeDefinition::Float32)
                || matches!(right, TypeDefinition::Float32)
            {
                return TypeDefinition::Float64;
            }
            return TypeDefinition::Int64;
        }

        match (left, right) {
            (TypeDefinition::List(a), TypeDefinition::List(b)) => {
                TypeDefinition::List(Box::new(Self::unify_types(*a, *b)))
            }
            (TypeDefinition::Optional(a), TypeDefinition::Optional(b)) => {
                TypeDefinition::Optional(Box::new(Self::unify_types(*a, *b)))
            }
            (TypeDefinition::Optional(a), other) | (other, TypeDefinition::Optional(a)) => {
                TypeDefinition::Optional(Box::new(Self::unify_types(*a, other)))
            }
            _ => TypeDefinition::String,
        }
    }

    fn is_numeric_type(type_def: &crate::schema::TypeDefinition) -> bool {
        use crate::schema::TypeDefinition;
        matches!(
            type_def,
            TypeDefinition::Int8
                | TypeDefinition::Int16
                | TypeDefinition::Int32
                | TypeDefinition::Int64
                | TypeDefinition::UInt8
                | TypeDefinition::UInt16
                | TypeDefinition::UInt32
                | TypeDefinition::UInt64
                | TypeDefinition::Float32
                | TypeDefinition::Float64
        )
    }

    /// Encode a value with a schema to produce a complete BCS file
    fn encode_value_with_schema(&mut self, value: &Value, schema: &Schema) -> Result<Vec<u8>> {
        // Reset buffers
        self.reset();

        // Build optional string table before writing data.
        let table = crate::string_table::StringTable::from_value(
            value,
            self.config.dedup,
            self.config.dedup_thresholds,
        );
        let use_dedup = !table.is_empty();
        self.string_table = if use_dedup {
            Some(table.as_arc())
        } else {
            None
        };

        // Encode the value to the data layer.
        // If index table is enabled, keep top-level field offsets for O(1) lookup.
        // Otherwise encode root as a single value for minimum overhead.
        if self.config.include_index_table {
            if let Value::Struct(fields) = value {
                for (field_name, _hash, field_value) in fields {
                    let offset = self.write_value(field_value)?;
                    self.track_offset(field_name.clone(), offset);
                    self.maybe_index_nested(field_name, field_value, offset)?;
                }
            } else {
                self.write_value(value)?;
            }
        } else {
            self.write_value(value)?;
        }

        // Build the semantic layer
        let semantic_data = if self.config.include_semantic_layer {
            self.build_semantic_layer(schema)?
        } else {
            Vec::new()
        };

        // Build index table
        let mut index_buffer = Vec::new();
        if self.config.include_index_table {
            let index_table = self.build_index_table()?;
            index_table.write(&mut index_buffer)?;
        }

        // String table section (between index and data)
        let string_table_bytes = if use_dedup {
            self.string_table
                .as_ref()
                .unwrap()
                .to_bytes()?
        } else {
            Vec::new()
        };

        // Build data layer with optional smart compression.
        // If compression is enabled, keep compressed bytes only when they are smaller.
        let (data_layer, data_compressed) = if self.config.data_compression {
            let compressed = lz4::block::compress(&self.data_buffer, None, true)
                .map_err(|e| BCSError::Encoding(format!("LZ4 data compression failed: {}", e)))?;

            if compressed.len() < self.data_buffer.len() {
                (compressed, true)
            } else {
                (self.data_buffer.clone(), false)
            }
        } else {
            (self.data_buffer.clone(), false)
        };

        // Calculate offsets
        let semantic_offset = HEADER_SIZE as u64;
        let semantic_size = semantic_data.len() as u64;
        let index_offset = semantic_offset + semantic_size;
        let index_size = index_buffer.len() as u64;
        let string_table_offset = index_offset + index_size;
        let string_table_size = string_table_bytes.len() as u64;
        let data_offset = string_table_offset + string_table_size;
        let data_size = data_layer.len() as u64;

        // Create header
        let mut header = Header::new();
        header.flags.compressed = self.config.compression && !semantic_data.is_empty();
        // Reserved bit 0x0002: writers must clear (ignore EncoderConfig.ai_metadata).
        header.flags.ai_metadata = false;
        header.flags.data_compressed = data_compressed;
        header.flags.structural_dedup = use_dedup;
        header.semantic_offset = semantic_offset;
        header.semantic_size = semantic_size;
        header.index_offset = index_offset;
        header.index_size = index_size;
        header.data_offset = data_offset;
        header.data_size = data_size;

        // Assemble the complete file
        let mut output = Vec::new();

        // Write header (checksum will be calculated later)
        header.write(&mut output)?;

        // Write semantic layer
        output.extend_from_slice(&semantic_data);

        // Write index table
        output.extend_from_slice(&index_buffer);

        // Write string table
        output.extend_from_slice(&string_table_bytes);

        // Write data layer
        output.extend_from_slice(&data_layer);

        // Calculate and update checksum.
        // Checksum is calculated over everything except the checksum field itself.
        let mut data_to_check = Vec::new();
        data_to_check.extend_from_slice(&output[0..CHECKSUM_OFFSET_START]);
        data_to_check.extend_from_slice(&output[CHECKSUM_OFFSET_END..]);

        let checksum = Self::calculate_crc64(&data_to_check);

        // Update checksum in header (bytes 56-64)
        output[CHECKSUM_OFFSET_START..CHECKSUM_OFFSET_END].copy_from_slice(&checksum.to_le_bytes());

        self.string_table = None;
        Ok(output)
    }

    /// Register nested field offsets for large structs/maps when `index_maps_over` is set.
    fn maybe_index_nested(&mut self, parent: &str, value: &Value, parent_offset: u64) -> Result<()> {
        let Some(threshold) = self.config.index_maps_over else {
            return Ok(());
        };
        match value {
            Value::Struct(fields) if fields.len() >= threshold => {
                // Offsets are relative to the start of this struct encoding in the data layer.
                let slice = self.data_buffer[parent_offset as usize..].to_vec();
                let table = self.string_table.clone();
                let mut nested = Vec::new();
                for (name, _, _) in fields {
                    if let Ok(rel) =
                        crate::decoder::nested_struct_field_relative_offset_ex(&slice, name, table.clone())
                    {
                        nested.push((format!("{}.{}", parent, name), parent_offset + rel));
                    }
                }
                for (path, offset) in nested {
                    self.track_offset(path, offset);
                }
            }
            Value::Map(entries) if entries.len() >= threshold => {
                for (key, _) in entries {
                    if let Value::String(name) = key {
                        // Map local indexes: register path; offset stays at map root
                        // (map entries lack stable child offsets without a format change).
                        let path = format!("{}.{}", parent, name);
                        self.track_offset(path, parent_offset);
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

// ============================================================================
// YAML Encoding
// ============================================================================

impl Encoder {
    /// Encode from YAML string
    pub fn encode_from_yaml(&mut self, yaml: &str) -> Result<Vec<u8>> {
        // Parse YAML to serde_yaml::Value
        let yaml_value: serde_yaml::Value = serde_yaml::from_str(yaml)
            .map_err(|e| BCSError::Encoding(format!("Failed to parse YAML: {}", e)))?;

        // Convert YAML value to JSON value (they have similar structures)
        let json_value = Self::yaml_to_json(&yaml_value)?;

        // Convert to BCS Value
        let value = Self::json_to_value(&json_value)?;

        // Infer or use provided schema
        let schema = if let Some(ref s) = self.schema {
            s.clone()
        } else {
            Self::infer_schema(&value)?
        };

        // Validate if schema is provided
        if self.schema.is_some() {
            let engine = crate::schema::SchemaEngine::new();
            let validation_result = engine.validate(&value, &schema);
            if !validation_result.is_valid() {
                let errors: Vec<String> = validation_result
                    .errors
                    .iter()
                    .map(|e| format!("{}: {}", e.path, e.message))
                    .collect();
                return Err(BCSError::Validation(format!(
                    "Validation failed:\n{}",
                    errors.join("\n")
                )));
            }
        }

        // Encode the data
        self.encode_value_with_schema(&value, &schema)
    }

    /// Convert serde_yaml::Value to serde_json::Value
    fn yaml_to_json(yaml: &serde_yaml::Value) -> Result<serde_json::Value> {
        match yaml {
            serde_yaml::Value::Null => Ok(serde_json::Value::Null),
            serde_yaml::Value::Bool(b) => Ok(serde_json::Value::Bool(*b)),
            serde_yaml::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(serde_json::Value::Number(i.into()))
                } else if let Some(u) = n.as_u64() {
                    Ok(serde_json::Value::Number(u.into()))
                } else if let Some(f) = n.as_f64() {
                    serde_json::Number::from_f64(f)
                        .map(serde_json::Value::Number)
                        .ok_or_else(|| BCSError::Encoding("Invalid float in YAML".to_string()))
                } else {
                    Err(BCSError::Encoding("Invalid number in YAML".to_string()))
                }
            }
            serde_yaml::Value::String(s) => Ok(serde_json::Value::String(s.clone())),
            serde_yaml::Value::Sequence(seq) => {
                let arr: Result<Vec<serde_json::Value>> =
                    seq.iter().map(Self::yaml_to_json).collect();
                Ok(serde_json::Value::Array(arr?))
            }
            serde_yaml::Value::Mapping(map) => {
                let mut obj = serde_json::Map::new();
                for (key, val) in map {
                    // Convert key to string
                    let key_str = match key {
                        serde_yaml::Value::String(s) => s.clone(),
                        serde_yaml::Value::Number(n) => n.to_string(),
                        serde_yaml::Value::Bool(b) => b.to_string(),
                        _ => {
                            return Err(BCSError::Encoding(
                                "Invalid key type in YAML mapping".to_string(),
                            ))
                        }
                    };
                    let val_json = Self::yaml_to_json(val)?;
                    obj.insert(key_str, val_json);
                }
                Ok(serde_json::Value::Object(obj))
            }
            serde_yaml::Value::Tagged(tagged) => {
                // Handle tagged values by converting the inner value
                Self::yaml_to_json(&tagged.value)
            }
        }
    }
}

// ============================================================================
// TOML Encoding
// ============================================================================

impl Encoder {
    /// Encode from TOML string
    pub fn encode_from_toml(&mut self, toml_str: &str) -> Result<Vec<u8>> {
        // Parse TOML to toml::Value
        let toml_value: toml::Value = toml::from_str(toml_str)
            .map_err(|e| BCSError::Encoding(format!("Failed to parse TOML: {}", e)))?;

        // Convert TOML value to JSON value
        let json_value = Self::toml_to_json(&toml_value)?;

        // Convert to BCS Value
        let value = Self::json_to_value(&json_value)?;

        // Infer or use provided schema
        let schema = if let Some(ref s) = self.schema {
            s.clone()
        } else {
            Self::infer_schema(&value)?
        };

        // Validate if schema is provided
        if self.schema.is_some() {
            let engine = crate::schema::SchemaEngine::new();
            let validation_result = engine.validate(&value, &schema);
            if !validation_result.is_valid() {
                let errors: Vec<String> = validation_result
                    .errors
                    .iter()
                    .map(|e| format!("{}: {}", e.path, e.message))
                    .collect();
                return Err(BCSError::Validation(format!(
                    "Validation failed:\n{}",
                    errors.join("\n")
                )));
            }
        }

        // Encode the data
        self.encode_value_with_schema(&value, &schema)
    }

    /// Convert toml::Value to serde_json::Value
    fn toml_to_json(toml_val: &toml::Value) -> Result<serde_json::Value> {
        match toml_val {
            toml::Value::String(s) => Ok(serde_json::Value::String(s.clone())),
            toml::Value::Integer(i) => Ok(serde_json::Value::Number((*i).into())),
            toml::Value::Float(f) => serde_json::Number::from_f64(*f)
                .map(serde_json::Value::Number)
                .ok_or_else(|| BCSError::Encoding("Invalid float in TOML".to_string())),
            toml::Value::Boolean(b) => Ok(serde_json::Value::Bool(*b)),
            toml::Value::Datetime(dt) => {
                // Convert datetime to string
                Ok(serde_json::Value::String(dt.to_string()))
            }
            toml::Value::Array(arr) => {
                let json_arr: Result<Vec<serde_json::Value>> =
                    arr.iter().map(Self::toml_to_json).collect();
                Ok(serde_json::Value::Array(json_arr?))
            }
            toml::Value::Table(table) => {
                let mut obj = serde_json::Map::new();
                for (key, val) in table {
                    let val_json = Self::toml_to_json(val)?;
                    obj.insert(key.clone(), val_json);
                }
                Ok(serde_json::Value::Object(obj))
            }
        }
    }
}

// ============================================================================
// Checksum Calculation
// ============================================================================

/// CRC64 polynomial (ECMA-182)
const CRC64_POLY: u64 = 0xC96C5795D7870F42;

/// CRC64 lookup table, cached for the process lifetime.
struct Crc64Table {
    table: [u64; 256],
}

impl Crc64Table {
    /// Return a reference to the process-wide CRC64 lookup table.
    fn get() -> &'static Self {
        use std::sync::OnceLock;
        static INSTANCE: OnceLock<Crc64Table> = OnceLock::new();
        INSTANCE.get_or_init(Self::build)
    }

    /// Build the CRC64 lookup table (called once).
    fn build() -> Self {
        let mut table = [0u64; 256];
        for (i, item) in table.iter_mut().enumerate() {
            let mut crc = i as u64;
            for _ in 0..8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ CRC64_POLY;
                } else {
                    crc >>= 1;
                }
            }
            *item = crc;
        }
        Self { table }
    }

    /// Calculate CRC64 checksum
    fn checksum(&self, data: &[u8]) -> u64 {
        let mut crc = 0xFFFFFFFFFFFFFFFFu64;
        for &byte in data {
            let index = ((crc ^ byte as u64) & 0xFF) as usize;
            crc = (crc >> 8) ^ self.table[index];
        }
        !crc
    }
}

impl Encoder {
    /// Calculate CRC64 checksum over file sections (excluding header checksum field)
    pub fn calculate_crc64(data: &[u8]) -> u64 {
        Crc64Table::get().checksum(data)
    }

    /// Verify checksum of a BCS file
    pub fn verify_checksum(file_data: &[u8]) -> Result<bool> {
        if file_data.len() < HEADER_SIZE {
            return Err(BCSError::Format("File too small".to_string()));
        }

        // Extract checksum from header (bytes 56-64)
        let stored_checksum = u64::from_le_bytes(
            file_data[CHECKSUM_OFFSET_START..CHECKSUM_OFFSET_END]
                .try_into()
                .map_err(|_| BCSError::Format("Invalid checksum field".to_string()))?,
        );

        // Calculate checksum over everything except the checksum field
        // This includes: header before checksum + content after checksum.
        let mut data_to_check = Vec::new();
        data_to_check.extend_from_slice(&file_data[0..CHECKSUM_OFFSET_START]);
        data_to_check.extend_from_slice(&file_data[CHECKSUM_OFFSET_END..]);

        let calculated_checksum = Self::calculate_crc64(&data_to_check);

        Ok(stored_checksum == calculated_checksum)
    }
}

// ============================================================================
// Convenience Methods
// ============================================================================

impl Encoder {
    /// Encode from a file (auto-detects format based on extension)
    pub fn encode_from_file(&mut self, path: &str) -> Result<Vec<u8>> {
        let content = std::fs::read_to_string(path).map_err(BCSError::Io)?;

        // Detect format from extension
        if path.ends_with(".json") {
            self.encode_from_json(&content)
        } else if path.ends_with(".yaml") || path.ends_with(".yml") {
            self.encode_from_yaml(&content)
        } else if path.ends_with(".toml") {
            self.encode_from_toml(&content)
        } else {
            Err(BCSError::Encoding(format!(
                "Unknown file format for: {}. Supported: .json, .yaml, .yml, .toml",
                path
            )))
        }
    }

    /// Encode and write to a file
    pub fn encode_to_file(&mut self, input_path: &str, output_path: &str) -> Result<()> {
        let bcs_data = self.encode_from_file(input_path)?;
        std::fs::write(output_path, bcs_data).map_err(BCSError::Io)?;
        Ok(())
    }

    /// Encode from JSON and write to file
    pub fn encode_json_to_file(&mut self, json: &str, output_path: &str) -> Result<()> {
        let bcs_data = self.encode_from_json(json)?;
        std::fs::write(output_path, bcs_data).map_err(BCSError::Io)?;
        Ok(())
    }

    /// Encode from YAML and write to file
    pub fn encode_yaml_to_file(&mut self, yaml: &str, output_path: &str) -> Result<()> {
        let bcs_data = self.encode_from_yaml(yaml)?;
        std::fs::write(output_path, bcs_data).map_err(BCSError::Io)?;
        Ok(())
    }

    /// Encode from TOML and write to file
    pub fn encode_toml_to_file(&mut self, toml: &str, output_path: &str) -> Result<()> {
        let bcs_data = self.encode_from_toml(toml)?;
        std::fs::write(output_path, bcs_data).map_err(BCSError::Io)?;
        Ok(())
    }
}

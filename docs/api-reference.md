# BCS Core API Reference

This document provides a comprehensive reference for the `bcs-core` Rust library.

## Table of Contents

- [Overview](#overview)
- [Types](#types)
- [Encoder](#encoder)
- [Decoder](#decoder)
- [Schema](#schema)
- [Index](#index)
- [Security](#security)
- [Error Handling](#error-handling)

---

## Overview

The `bcs-core` library provides the core functionality for encoding, decoding, and validating Binary Config Schema (BCS) files.

```rust
use bcs_core::{Encoder, Decoder, Schema, BCSError};
```

---

## Types

### `Value`

The core enum representing all possible values in BCS.

```rust
pub enum Value {
    Null,
    Bool(bool),
    Int8(i8),
    Int16(i16),
    Int32(i32),
    Int64(i64),
    UInt8(u8),
    UInt16(u16),
    UInt32(u32),
    UInt64(u64),
    Float32(f32),
    Float64(f64),
    String(String),
    Bytes(Vec<u8>),
    List(Vec<Value>),
    Map(Vec<(Value, Value)>),
    Struct(Vec<(String, u64, Value)>),  // (field_name, hash, value)
    Union(u32, Box<Value>),             // (variant_tag, value)
    Optional(Option<Box<Value>>),
}
```

### `TypeTag`

Type tags used in the binary encoding (1 byte each).

| Tag | Type |
|-----|------|
| `0x00` | null |
| `0x01` | bool false |
| `0x02` | bool true |
| `0x10..0x17` | integers (signed/unsigned, 8/16/32/64 bit) |
| `0x20` | float32 |
| `0x21` | float64 |
| `0x30` | string inline (len < 256) |
| `0x31` | string external (len >= 256) |
| `0x32` | bytes inline |
| `0x33` | bytes external |
| `0x40` | list |
| `0x41` | map |
| `0x42` | struct |
| `0x43` | union |
| `0x44` | optional some |
| `0x45` | optional none |

### `Header`

BCS file header (64 bytes fixed size).

```rust
pub struct Header {
    pub magic: u32,           // 0x42435346 ("BCSF")
    pub version_major: u8,    // currently 1
    pub version_minor: u8,    // currently 0
    pub flags: HeaderFlags,
    pub semantic_offset: u64,
    pub semantic_size: u64,
    pub index_offset: u64,
    pub index_size: u64,
    pub data_offset: u64,
    pub data_size: u64,
    pub checksum: u64,        // CRC64
}
```

### `HeaderFlags`

```rust
pub struct HeaderFlags {
    pub compressed: bool,      // semantic layer LZ4 compressed
    pub ai_metadata: bool,     // reserved bit 0x0002 (writers clear; readers ignore)
    pub data_compressed: bool, // data layer LZ4 compressed
}
```

---

## Encoder

### `EncoderConfig`

Configuration options for the encoder.

```rust
pub struct EncoderConfig {
    pub compression: bool,              // LZ4 compress semantic layer
    pub ai_metadata: bool,              // reserved/ignored (bit 0x0002 never written)
    pub include_semantic_layer: bool,   // Embed schema
    pub include_index_table: bool,      // Include index for O(1) lookup
    pub data_compression: bool,         // LZ4 compress data layer
}
```

### `Encoder`

Main encoder for converting structured data to BCS format.

#### Creating an Encoder

```rust
use bcs_core::{Encoder, EncoderConfig};

// Default configuration
let encoder = Encoder::new();

// Custom configuration
let config = EncoderConfig {
    compression: true,
    include_semantic_layer: true,
    include_index_table: true,
    ..Default::default()
};
let encoder = Encoder::with_config(config);

// With schema validation
let encoder = Encoder::new().with_schema(schema);
```

#### Encoding Methods

```rust
// Encode from JSON string
let bcs_bytes: Vec<u8> = encoder.encode_from_json(json_str)?;

// Encode from YAML string
let bcs_bytes: Vec<u8> = encoder.encode_from_yaml(yaml_str)?;

// Encode from TOML string
let bcs_bytes: Vec<u8> = encoder.encode_from_toml(toml_str)?;

// Encode from file (auto-detects format)
let bcs_bytes: Vec<u8> = encoder.encode_from_file("config.json")?;

// Encode and write to file
encoder.encode_to_file("config.json", "config.bcs")?;
```

#### Configuration Methods

```rust
encoder.set_compression(true);
// set_ai_metadata is deprecated/no-op (reserved header bit)
encoder.set_include_semantic_layer(true);
encoder.set_include_index_table(true);
encoder.set_compact_mode(false);
encoder.set_data_compression(true);
```

---

## Decoder

### `Decoder`

Main decoder for reading BCS binary format.

#### Creating a Decoder

```rust
use bcs_core::Decoder;

// From file path
let mut decoder = Decoder::from_file("config.bcs")?;

// From file with memory-mapped I/O (better for large files)
let mut decoder = Decoder::from_file_mmap("config.bcs")?;

// From byte buffer
let mut decoder = Decoder::from_bytes(&bcs_bytes)?;
```

#### Decoding Methods

```rust
// Decode to BCS Value
let value: Value = decoder.decode_to_value()?;

// Convert to JSON string
let json: String = decoder.to_json()?;

// Convert to YAML string
let yaml: String = decoder.to_yaml()?;

// Get header information
let header: &Header = decoder.header();

// Get schema (if present)
let schema: &Schema = decoder.schema()?;

// Get index table (if present)
let index: &IndexTable = decoder.index_table()?;

// Get file metadata
let metadata: FileMetadata = decoder.metadata();
```

#### Path Query Methods

```rust
// Get value at path
let value: Value = decoder.get("database.host")?;

// Get value at path with wildcard
let values: Value = decoder.get_wildcard("services.$.routes")?;

// Get value with offset information
let (offset, value): (u64, Value) = decoder.get_with_offset("server.port")?;
```

#### Path Syntax

BCS supports flexible path syntax:

```rust
// Dot notation for object fields
decoder.get("database.host")?;

// Bracket notation for array indices
decoder.get("servers[0]")?;

// Combined nesting
decoder.get("services[0].routes[1].method")?;

// Wildcard queries (Mongo-style)
decoder.get("services.$.routes")?;
```

---

## Schema

### `Schema`

Schema definition for BCS files.

```rust
use bcs_core::schema::{Schema, TypeDefinition, FieldDefinition, Constraint};

// Create a new schema
let mut schema = Schema::new("Root".to_string());

// Add type definitions
schema.add_type("Root".to_string(), TypeDefinition::Struct(fields));
schema.add_type("Database".to_string(), TypeDefinition::Struct(db_fields));

// Add constraints
schema.add_constraints("database.port".to_string(), vec![
    Constraint::Range(1.0, 65535.0),
]);

// Add documentation
schema.add_documentation("database.host".to_string(), 
    "Database server hostname".to_string());

// Serialize to MessagePack
let bytes: Vec<u8> = schema.to_msgpack()?;

// Deserialize from MessagePack
let schema: Schema = Schema::from_msgpack(&bytes)?;
```

### `TypeDefinition`

Enum representing all possible types in the schema.

```rust
pub enum TypeDefinition {
    // Primitive types
    Int8, Int16, Int32, Int64,
    UInt8, UInt16, UInt32, UInt64,
    Float32, Float64,
    Bool, String, Bytes, Null,

    // Composite types
    List(Box<TypeDefinition>),
    Map(Box<TypeDefinition>, Box<TypeDefinition>),
    Struct(HashMap<String, FieldDefinition>),
    Union(Vec<VariantDefinition>),
    Optional(Box<TypeDefinition>),

    // Custom type reference
    Custom(String),
}
```

### `FieldDefinition`

```rust
pub struct FieldDefinition {
    pub field_type: TypeDefinition,
    pub required: bool,
    pub default: Option<Value>,
    pub constraints: Vec<Constraint>,
    pub documentation: Option<String>,
    pub ai_tag: Option<String>,
}
```

### `Constraint`

Validation constraints for fields.

```rust
pub enum Constraint {
    // Numeric constraints
    Range(f64, f64),      // min, max
    Min(f64),              // minimum value
    Max(f64),              // maximum value

    // String constraints
    Pattern(String),       // regex pattern
    NonEmpty,              // must not be empty
    Length(Option<usize>, Option<usize>),  // min, max length

    // Collection constraints
    Unique,                // unique elements in list

    // Enum constraint
    Enum(Vec<Value>),      // must be one of these values

    // Custom constraint (reserved)
    Custom(String),
}
```

#### Pattern Constraint Examples

```rust
// Email pattern
Constraint::Pattern(r"^[\w.-]+@[\w.-]+\.\w+$".to_string())

// Phone number (E.164)
Constraint::Pattern(r"^\+?[1-9]\d{1,14}$".to_string())

// IPv4 address
Constraint::Pattern(r"^((25[0-5]|2[0-4]\d|[01]?\d\d?)\.){3}(25[0-5]|2[0-4]\d|[01]?\d\d?)$".to_string())

// Alphanumeric 8+ characters
Constraint::Pattern(r"^[a-zA-Z0-9]{8,}$".to_string())
```

### `SchemaEngine`

Engine for validating values against schemas.

```rust
use bcs_core::schema::SchemaEngine;

let engine = SchemaEngine::new();

// Register custom types
engine.register_custom_type("User".to_string(), user_type);

// Validate a value
let result = engine.validate(&value, &schema);
if !result.is_valid() {
    for error in &result.errors {
        println!("{}: {}", error.path, error.message);
    }
}
```

---

## Index

### `IndexTable`

Hash table for O(1) random access to configuration values.

```rust
use bcs_core::index::IndexTable;

// Get all entries
let entries: Vec<(String, u64)> = index_table.entries();

// Get entry count
let count: usize = index_table.len();

// Check if empty
let empty: bool = index_table.is_empty();

// Get statistics
let stats: IndexStats = index_table.stats();
```

### `IndexTableBuilder`

Builder for constructing index tables.

```rust
use bcs_core::index::IndexTableBuilder;

let mut builder = IndexTableBuilder::new();
builder.add_entry("database.host".to_string(), offset);
builder.add_entry("server.port".to_string(), offset);

let index_table: IndexTable = builder.build()?;
```

### Path Parsing

```rust
use bcs_core::index::parse_path;

let segments = parse_path("services[0].routes[1].paths[0]")?;
// Returns: [Field("services"), Index(0), Field("routes"), Index(1), Field("paths"), Index(0)]
```

---

## Security

### Protecting Sensitive Fields

```rust
use bcs_core::security::{
    protect_paths, protect_paths_kms, reveal_paths, reveal_all, reveal_all_ex, mask_all,
    mask_secret_refs, mask_sensitive_fields, resolve_secret_refs, is_protected_marker,
    is_secret_ref_marker, format_secret_ref, parse_secret_ref, ResolverRegistry, KeyWrapper,
};

// Password scheme (`pbkdf2`): PBKDF2-HMAC-SHA256 (120k iters) + AES-256-GCM
protect_paths(&mut value, &["database.password".to_string()], "secret")?;

// Reveal specific paths
reveal_paths(&mut value, &["database.password".to_string()], "secret")?;

// Reveal all pbkdf2-protected fields
reveal_all(&mut value, "secret")?;

// KMS scheme (`kms`): random DEK + AES-256-GCM; DEK wrap via host KeyWrapper
// Mixed trees (pbkdf2 + kms) use reveal_all_ex
protect_paths_kms(&mut value, &["api.token".to_string()], "cmd", "alias/app", &wrapper)?;
reveal_all_ex(&mut value, Some("secret"), Some(&wrapper))?;

// Mask all protected fields (without decrypting)
mask_all(&mut value)?;

// Check if a value is protected
let is_protected: bool = is_protected_marker(&value);
```

Schemes use distinct marker prefixes (`__bcs_sensitive_pbkdf2__:` /
`__bcs_sensitive_kms__:`) — payload layouts and CLI KMS providers are in
[identity.md](identity.md).

### Secret References

Store a marker string instead of the secret value. Resolution is pluggable via
[`SecretResolver`](../core/src/secret_resolver.rs) / `ResolverRegistry`.

```rust
use bcs_core::security::{
    format_secret_ref, is_secret_ref_marker, mask_secret_refs, mask_sensitive_fields,
    parse_secret_ref, resolve_secret_refs, ResolverRegistry, SecretResolver,
};
use std::sync::Arc;

// Build a marker: __bcs_secret_ref__:env:API_TOKEN
let marker = format_secret_ref("env", "API_TOKEN")?;

// Detect / parse (any valid scheme; providers checked at resolve time)
assert!(is_secret_ref_marker(&Value::String(marker.clone())));
let parsed = parse_secret_ref(&marker)?;

// Mask without resolving
mask_secret_refs(&mut value)?;

// Resolve with the built-in env registry (`secret:` remaps to env by default)
resolve_secret_refs(&mut value, &ResolverRegistry::with_env())?;

// Or register a custom provider
struct FakeResolver;
impl SecretResolver for FakeResolver {
    fn resolve(&self, scheme: &str, locator: &str) -> bcs_core::Result<String> {
        Ok(format!("{scheme}={locator}"))
    }
}
let mut registry = ResolverRegistry::new();
registry.register("vault", Arc::new(FakeResolver));
resolve_secret_refs(&mut value, &registry)?;

// Mask both password-protected markers and secret refs
mask_sensitive_fields(&mut value)?;
```

CLI: `bcs decode file.bcs --resolve-secrets [--secret-provider <name>]`
(or `BCS_SECRET_PROVIDER`). See [secrets.md](secrets.md) for providers, auth, and build features.
Prefer ephemeral identity over static tokens: [identity.md](identity.md).
---

## Error Handling

### `BCSError`

All errors in BCS are represented by the `BCSError` enum.

```rust
pub enum BCSError {
    Io(std::io::Error),
    Format(String),        // Invalid file format
    Decoding(String),      // Decoding errors
    Encoding(String),      // Encoding errors
    Validation(String),    // Validation errors
}
```

### `Result<T>`

```rust
pub type Result<T> = std::result::Result<T, BCSError>;
```

### Example Error Handling

```rust
use bcs_core::{Decoder, BCSError};

match Decoder::from_file("config.bcs") {
    Ok(mut decoder) => {
        match decoder.decode_to_value() {
            Ok(value) => println!("Decoded: {:?}", value),
            Err(BCSError::Decoding(msg)) => eprintln!("Decoding error: {}", msg),
            Err(BCSError::Validation(msg)) => eprintln!("Validation error: {}", msg),
            Err(e) => eprintln!("Other error: {}", e),
        }
    }
    Err(BCSError::Io(e)) => eprintln!("IO error: {}", e),
    Err(BCSError::Format(msg)) => eprintln!("Format error: {}", msg),
    Err(e) => eprintln!("Error: {}", e),
}
```

---

## Examples

### Basic Encode/Decode

```rust
use bcs_core::{Encoder, Decoder};

fn main() -> bcs_core::Result<()> {
    // Encode JSON to BCS
    let json = r#"{"name": "test", "value": 42}"#;
    let mut encoder = Encoder::new();
    let bcs_bytes = encoder.encode_from_json(json)?;

    // Decode BCS to JSON
    let mut decoder = Decoder::from_bytes(&bcs_bytes)?;
    let output_json = decoder.to_json()?;

    println!("Output: {}", output_json);
    Ok(())
}
```

### Schema Validation

```rust
use bcs_core::{Encoder, Schema, SchemaEngine};
use bcs_core::schema::{TypeDefinition, FieldDefinition, Constraint};
use std::collections::HashMap;

fn main() -> bcs_core::Result<()> {
    // Create schema
    let mut fields = HashMap::new();
    fields.insert("name".to_string(), FieldDefinition {
        field_type: TypeDefinition::String,
        required: true,
        default: None,
        constraints: vec![Constraint::NonEmpty],
        documentation: None,
        ai_tag: None,
    });

    let mut schema = Schema::new("Root".to_string());
    schema.add_type("Root".to_string(), TypeDefinition::Struct(fields));

    // Encode with schema
    let json = r#"{"name": "test"}"#;
    let mut encoder = Encoder::new().with_schema(schema);
    let bcs_bytes = encoder.encode_from_json(json)?;

    Ok(())
}
```

### Path Queries

```rust
use bcs_core::Decoder;

fn main() -> bcs_core::Result<()> {
    let mut decoder = Decoder::from_file("config.bcs")?;

    // Simple path query
    let host = decoder.get("database.host")?;

    // Array index query
    let first_server = decoder.get("servers[0]")?;

    // Nested query
    let route = decoder.get("services[0].routes[1].method")?;

    // Wildcard query
    let all_routes = decoder.get("services.$.routes")?;

    Ok(())
}
```

---

## Features

| Feature | Description |
|---------|-------------|
| `compression` | LZ4 compression for semantic layer |
| `data_compression` | LZ4 compression for data layer |
| `index_table` | O(1) random access via hash table |
| `semantic_layer` | Embedded schema for type information |
| `memory_mapped` | Zero-copy file access via mmap |
| `validation` | Schema-based value validation |
| `security` | Password-based field encryption |

---

## Performance

BCS is designed for high-performance configuration access:

- **O(1) random access** via embedded index table
- **Zero-copy reads** with memory-mapped I/O
- **LZ4 compression** for reduced file size
- **Compact binary format** with minimal overhead

See `docs/performance-gates.md` for benchmarking methodology and regression thresholds.

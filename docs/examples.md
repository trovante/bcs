# BCS Examples

This document provides practical examples of using BCS for various use cases.

## Table of Contents

- [Quick Start](#quick-start)
- [Basic Usage](#basic-usage)
- [Schema Validation](#schema-validation)
- [Path Queries](#path-queries)
- [Security](#security)
- [Advanced Usage](#advanced-usage)
- [CLI Examples](#cli-examples)

---

## Quick Start

### Minimal Encode/Decode

```rust
use bcs_core::{Encoder, Decoder};

fn main() -> bcs_core::Result<()> {
    // Encode JSON to BCS
    let json = r#"{"name": "app", "version": "1.0"}"#;
    let mut encoder = Encoder::new();
    let bcs_bytes = encoder.encode_from_json(json)?;

    // Write to file
    std::fs::write("config.bcs", &bcs_bytes)?;

    // Decode back to JSON
    let mut decoder = Decoder::from_file("config.bcs")?;
    let output = decoder.to_json()?;

    println!("Decoded: {}", output);
    Ok(())
}
```

### Using YAML

```rust
use bcs_core::{Encoder, Decoder};

fn main() -> bcs_core::Result<()> {
    let yaml = r#"
database:
  host: localhost
  port: 5432
  name: mydb
servers:
  - host: server1.example.com
    port: 8080
  - host: server2.example.com
    port: 8081
"#;

    let mut encoder = Encoder::new();
    let bcs_bytes = encoder.encode_from_yaml(yaml)?;

    let mut decoder = Decoder::from_bytes(&bcs_bytes)?;
    println!("{}", decoder.to_json()?);

    Ok(())
}
```

---

## Basic Usage

### Encoder Configuration

```rust
use bcs_core::{Encoder, EncoderConfig};

fn main() -> bcs_core::Result<()> {
    let json = r#"{"key": "value"}"#;

    // Default configuration
    let mut encoder = Encoder::new();
    let default_bcs = encoder.encode_from_json(json)?;

    // Compact mode (no schema, no index)
    let config = EncoderConfig {
        compression: false,
        include_semantic_layer: false,
        include_index_table: false,
        ..Default::default()
    };
    let mut encoder = Encoder::with_config(config);
    let compact_bcs = encoder.encode_from_json(json)?;

    // With data compression
    let mut encoder = Encoder::new();
    encoder.set_data_compression(true);
    let compressed_bcs = encoder.encode_from_json(json)?;

    println!("Default: {} bytes", default_bcs.len());
    println!("Compact: {} bytes", compact_bcs.len());
    println!("Compressed: {} bytes", compressed_bcs.len());

    Ok(())
}
```

### Decoder Options

```rust
use bcs_core::Decoder;

fn main() -> bcs_core::Result<()> {
    // Standard file reading
    let mut decoder = Decoder::from_file("config.bcs")?;

    // Memory-mapped I/O (better for large files)
    let mut decoder = Decoder::from_file_mmap("config.bcs")?;

    // From byte buffer
    let bytes = std::fs::read("config.bcs")?;
    let mut decoder = Decoder::from_bytes(&bytes)?;

    // Check if using mmap
    println!("Using mmap: {}", decoder.is_mmap());

    // Get file metadata
    let metadata = decoder.metadata();
    println!("Total size: {} bytes", metadata.total_size);

    Ok(())
}
```

---

## Schema Validation

### Creating a Schema

```rust
use bcs_core::schema::{Schema, TypeDefinition, FieldDefinition, Constraint};
use std::collections::HashMap;

fn create_app_schema() -> Schema {
    let mut schema = Schema::new("AppConfig".to_string());

    // Define DatabaseConfig type
    let mut db_fields = HashMap::new();
    db_fields.insert("host".to_string(), FieldDefinition {
        field_type: TypeDefinition::String,
        required: true,
        default: None,
        constraints: vec![Constraint::NonEmpty],
        documentation: Some("Database host".to_string()),
        ai_tag: None,
    });
    db_fields.insert("port".to_string(), FieldDefinition {
        field_type: TypeDefinition::UInt16,
        required: true,
        default: Some(bcs_core::types::Value::UInt16(5432)),
        constraints: vec![Constraint::Range(1.0, 65535.0)],
        documentation: Some("Database port".to_string()),
        ai_tag: None,
    });
    db_fields.insert("name".to_string(), FieldDefinition {
        field_type: TypeDefinition::String,
        required: true,
        default: None,
        constraints: vec![Constraint::NonEmpty],
        documentation: Some("Database name".to_string()),
        ai_tag: None,
    });

    schema.add_type("DatabaseConfig".to_string(), TypeDefinition::Struct(db_fields));

    // Define Server type
    let mut server_fields = HashMap::new();
    server_fields.insert("host".to_string(), FieldDefinition {
        field_type: TypeDefinition::String,
        required: true,
        default: None,
        constraints: vec![Constraint::NonEmpty],
        documentation: None,
        ai_tag: None,
    });
    server_fields.insert("port".to_string(), FieldDefinition {
        field_type: TypeDefinition::UInt16,
        required: true,
        default: None,
        constraints: vec![Constraint::Range(1.0, 65535.0)],
        documentation: None,
        ai_tag: None,
    });

    schema.add_type("Server".to_string(), TypeDefinition::Struct(server_fields));

    // Define root AppConfig type
    let mut app_fields = HashMap::new();
    app_fields.insert("database".to_string(), FieldDefinition {
        field_type: TypeDefinition::Custom("DatabaseConfig".to_string()),
        required: true,
        default: None,
        constraints: vec![],
        documentation: None,
        ai_tag: None,
    });
    app_fields.insert("servers".to_string(), FieldDefinition {
        field_type: TypeDefinition::List(Box::new(TypeDefinition::Custom("Server".to_string()))),
        required: true,
        default: None,
        constraints: vec![Constraint::NonEmpty],
        documentation: None,
        ai_tag: None,
    });

    schema.add_type("AppConfig".to_string(), TypeDefinition::Struct(app_fields));

    schema
}
```

### Using Schema with Encoder

```rust
use bcs_core::Encoder;

fn main() -> bcs_core::Result<()> {
    let schema = create_app_schema();

    let json = r#"{
        "database": {
            "host": "localhost",
            "port": 5432,
            "name": "mydb"
        },
        "servers": [
            {"host": "server1.example.com", "port": 8080},
            {"host": "server2.example.com", "port": 8081}
        ]
    }"#;

    let mut encoder = Encoder::new().with_schema(schema);
    let bcs_bytes = encoder.encode_from_json(json)?;

    std::fs::write("config.bcs", bcs_bytes)?;

    Ok(())
}
```

### Validating Values

```rust
use bcs_core::schema::{SchemaEngine, Schema, TypeDefinition, Constraint};

fn main() -> bcs_core::Result<()> {
    let schema = create_app_schema();
    let engine = SchemaEngine::new();

    // Parse JSON value
    let json_value: serde_json::Value = serde_json::from_str(r#"{
        "database": {
            "host": "localhost",
            "port": 5432,
            "name": "mydb"
        },
        "servers": [
            {"host": "server1.example.com", "port": 8080}
        ]
    }"#)?;

    // Convert to BCS Value
    let bcs_value = bcs_core::Encoder::json_to_value(&json_value)?;

    // Validate
    let result = engine.validate(&bcs_value, &schema);

    if result.is_valid() {
        println!("Validation passed!");
    } else {
        for error in &result.errors {
            println!("Error at {}: {}", error.path, error.message);
        }
    }

    Ok(())
}
```

---

## Path Queries

### Basic Path Queries

```rust
use bcs_core::Decoder;

fn main() -> bcs_core::Result<()> {
    let mut decoder = Decoder::from_file("config.bcs")?;

    // Simple field access
    let host = decoder.get("database.host")?;
    println!("Host: {:?}", host);

    // Array index
    let first_server = decoder.get("servers[0]")?;
    println!("First server: {:?}", first_server);

    // Nested access
    let port = decoder.get("servers[0].port")?;
    println!("Port: {:?}", port);

    Ok(())
}
```

### Wildcard Queries

```rust
use bcs_core::Decoder;

fn main() -> bcs_core::Result<()> {
    let mut decoder = Decoder::from_file("config.bcs")?;

    // Wildcard across arrays (Mongo-style)
    let all_hosts = decoder.get_wildcard("servers.$.host")?;
    println!("All hosts: {:?}", all_hosts);

    // Deep wildcard
    let all_ports = decoder.get_wildcard("servers.$.ports.$")?;
    println!("All ports: {:?}", all_ports);

    // Flatten results
    let flat_results = decoder.get_wildcard("servers.$.ports.$")?;
    println!("Flat results: {:?}", flat_results);

    Ok(())
}
```

### Complex Nested Structures

```rust
use bcs_core::Decoder;

fn main() -> bcs_core::Result<()> {
    let mut decoder = Decoder::from_file("kubernetes.bcs")?;

    // Access deeply nested values
    let container_image = decoder.get("spec.template.spec.containers[0].image")?;
    println!("Container image: {:?}", container_image);

    // Access array of objects
    let all_container_names = decoder.get_wildcard("spec.template.spec.containers.$.name")?;
    println!("Container names: {:?}", all_container_names);

    // Access with mixed notation
    let env_var = decoder.get("spec.template.spec.containers[0].env[0].value")?;
    println!("Env var: {:?}", env_var);

    Ok(())
}
```

---

## Security

### Protecting Sensitive Fields (`pbkdf2`)

```rust
use bcs_core::{Encoder, Decoder};
use bcs_core::security::{protect_paths, reveal_paths};

fn main() -> bcs_core::Result<()> {
    let json = r#"{
        "database": {
            "host": "localhost",
            "port": 5432,
            "password": "super_secret"
        },
        "api": {
            "token": "bearer_token_123"
        }
    }"#;

    // Encode to BCS
    let mut encoder = Encoder::new();
    let mut bcs_bytes = encoder.encode_from_json(json)?;

    // Parse as Value
    let mut value = Decoder::from_bytes(&bcs_bytes)?.decode_to_value()?;

    // Protect sensitive fields (PBKDF2-HMAC-SHA256 + AES-256-GCM)
    protect_paths(&mut value, &[
        "database.password".to_string(),
        "api.token".to_string(),
    ], "my_secret_password")?;

    // Encode protected value
    let protected_bcs = encoder.encode_from_json(&serde_json::to_string(&value)?)?;
    std::fs::write("config.secure.bcs", protected_bcs)?;

    // Later: reveal protected fields
    let mut decoder = Decoder::from_file("config.secure.bcs")?;
    let mut value = decoder.decode_to_value()?;

    // Without password: values are masked
    println!("Masked: {:?}", decoder.to_json()?);

    // With password: values are revealed
    reveal_paths(&mut value, &[
        "database.password".to_string(),
    ], "my_secret_password")?;

    let revealed_json = serde_json::to_string_pretty(&value)?;
    println!("{}", revealed_json);

    Ok(())
}
```

### Protecting Sensitive Fields (`kms`)

```rust
use bcs_core::security::{protect_paths_kms, reveal_all_ex, KeyWrapper};

// `wrapper` implements KeyWrapper (host callback or bcs-secrets native backend)
protect_paths_kms(
    &mut value,
    &["database.password".to_string()],
    "aws",
    "alias/app-key",
    &wrapper,
)?;

// Reveal: password for pbkdf2 markers + wrapper for kms markers
reveal_all_ex(&mut value, Some("my_secret_password"), Some(&wrapper))?;
```

See [identity.md](identity.md) for payload layouts and CLI KMS providers.

---

## Advanced Usage

### Custom Value Construction

```rust
use bcs_core::types::Value;
use bcs_core::Encoder;

fn main() -> bcs_core::Result<()> {
    // Construct BCS Value manually
    let value = Value::Struct(vec![
        ("name".to_string(), 0, Value::String("test".to_string())),
        ("count".to_string(), 0, Value::Int32(42)),
        ("tags".to_string(), 0, Value::List(vec![
            Value::String("rust".to_string()),
            Value::String("bcs".to_string()),
        ])),
        ("metadata".to_string(), 0, Value::Map(vec![
            (Value::String("version".to_string()), Value::String("1.0".to_string())),
        ])),
    ]);

    // Encode the value
    let mut encoder = Encoder::new();
    let bcs_bytes = encoder.encode_from_json(&serde_json::to_string(&value)?)?;

    Ok(())
}
```

### Working with Bytes

```rust
use bcs_core::types::Value;
use bcs_core::{Encoder, Decoder};

fn main() -> bcs_core::Result<()> {
    // Create value with bytes
    let value = Value::Struct(vec![
        ("name".to_string(), 0, Value::String("binary_data".to_string())),
        ("data".to_string(), 0, Value::Bytes(vec![0x48, 0x65, 0x6C, 0x6C, 0x6F])),
    ]);

    // Encode
    let json = serde_json::to_string(&value)?;
    let mut encoder = Encoder::new();
    let bcs_bytes = encoder.encode_from_json(&json)?;

    // Decode
    let mut decoder = Decoder::from_bytes(&bcs_bytes)?;
    let decoded = decoder.decode_to_value()?;

    if let Value::Struct(fields) = decoded {
        for (name, _, val) in fields {
            if name == "data" {
                if let Value::Bytes(bytes) = val {
                    println!("Bytes: {:?}", bytes);
                    println!("As string: {}", String::from_utf8_lossy(&bytes));
                }
            }
        }
    }

    Ok(())
}
```

### Schema Serialization

```rust
use bcs_core::schema::Schema;

fn main() -> bcs_core::Result<()> {
    let schema = create_app_schema();

    // Serialize to MessagePack
    let msgpack_bytes = schema.to_msgpack()?;
    println!("Schema size: {} bytes", msgpack_bytes.len());

    // Deserialize from MessagePack
    let restored_schema = Schema::from_msgpack(&msgpack_bytes)?;

    // Verify
    assert_eq!(schema.root, restored_schema.root);
    assert_eq!(schema.types.len(), restored_schema.types.len());

    Ok(())
}
```

---

## CLI Examples

### Basic Operations

```bash
# Encode JSON to BCS
bcs encode config.json -o config.bcs

# Encode YAML to BCS
bcs encode config.yaml -o config.bcs

# Decode BCS to JSON
bcs decode config.bcs -o config.json

# Validate BCS file
bcs validate config.bcs

# Inspect BCS file
bcs inspect config.bcs --verbose
```

### Size Profiles

```bash
# Default profile (schema + index + data)
bcs encode config.json -o config.default.bcs

# Compact profile (data only)
bcs encode config.json -o config.compact.bcs --compact

# With data compression
bcs encode config.json -o config.compressed.bcs --compress-data

# Compact + compressed
bcs encode config.json -o config.compact.compressed.bcs --compact --compress-data
```

### Path Queries

```bash
# Simple field query
bcs decode config.bcs --path database.host

# Array index query
bcs decode config.bcs --path servers[0]

# Nested query
bcs decode config.bcs --path services[0].routes[1].method

# Wildcard query
bcs decode config.bcs --path servers.$.host

# Wildcard with flatten
bcs decode config.bcs --path servers.$.ports.$ --path-flatten
```

### Security

```bash
# Protect fields during encode (pbkdf2, default)
bcs encode config.json -o config.secure.bcs \
  --protect-paths "database.password,api.token" \
  --protect-password "my-secret"

# Protect existing BCS file
bcs protect config.bcs -o config.protected.bcs \
  --paths "database.password" \
  --password "my-secret"

# Protect with KMS envelope (build CLI with secrets-aws, etc.)
bcs protect config.bcs -o config.kms.bcs \
  --paths "database.password" \
  --scheme kms --kms-provider aws --kms-key alias/app-key

# Decode without credentials (masked)
bcs decode config.secure.bcs

# Decode with password (pbkdf2 revealed)
bcs decode config.secure.bcs --password "my-secret"

# Decode with KMS unwrap
bcs decode config.kms.bcs --unwrap-kms --kms-provider aws
```

### Benchmarking

```bash
# Basic benchmark
bcs benchmark config.bcs

# Compare with JSON
bcs benchmark config.bcs --compare config.json

# Custom run count
bcs benchmark config.bcs --runs 20

# Path hot-loop benchmark
bcs benchmark config.bcs --mode path-hot --runs 20

# JSON output for CI
bcs benchmark config.bcs --json | jq '.bcs.decode_time_p95_ns'
```

### CI/CD Integration

```bash
# Validate and check with jq
bcs validate config.bcs --json | jq -e '.ok == true' >/dev/null

# Get file size
bcs inspect config.bcs --json | jq '.metadata.total_size'

# Track benchmark metrics
bcs benchmark config.bcs --json | jq '.bcs.decode_time_p95_ns'

# Preview reindex impact
bcs reindex compact.bcs --dry-run --json | jq '.projected_output_size'
```

---

## Tips and Best Practices

### 1. Choose the Right Profile

- **Default**: Use when you need schema validation and fast path queries
- **Compact**: Use when file size is critical and you don't need schema/index
- **Compressed**: Use for large files where compression helps

### 2. Schema Design

- Define schemas for complex configurations
- Use constraints to validate data at encode time
- Add documentation for maintainability

### 3. Path Queries

- Use wildcards for array traversals
- Combine with `--path-flatten` for flat results
- Remember: path queries require index table and uncompressed data

### 4. Security

- Use environment variables for passwords in CI
- Prefer `kms` protect or secret refs in production; keep `pbkdf2` for offline/shared-password cases
- Always use strong passwords when using the `pbkdf2` scheme
- Prefer OIDC/IAM/workload identity for KMS and secret providers ([identity.md](identity.md))

### 5. Performance

- Use memory-mapped I/O for large files
- Run benchmarks to compare profiles
- Monitor regression gates in CI

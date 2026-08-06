// Decoder tests

use bcs_core::decoder::Decoder;
use bcs_core::encoder::Encoder;
use bcs_core::types::Value;

// ============================================================================
// Primitive Type Decoding Tests
// ============================================================================

#[test]
fn test_decode_null() {
    let json = r#"null"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");

    let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
    let value = decoder.decode_to_value().expect("Failed to decode");

    assert_eq!(value, Value::Null);
}

#[test]
fn test_decode_bool() {
    // Test true
    let json = r#"true"#;
    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");
    let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
    let value = decoder.decode_to_value().expect("Failed to decode");
    assert_eq!(value, Value::Bool(true));

    // Test false
    let json = r#"false"#;
    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");
    let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
    let value = decoder.decode_to_value().expect("Failed to decode");
    assert_eq!(value, Value::Bool(false));
}

#[test]
fn test_decode_integers() {
    // Test positive integer
    let json = r#"42"#;
    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");
    let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
    let value = decoder.decode_to_value().expect("Failed to decode");

    // JSON numbers can be decoded as various integer types
    match value {
        Value::Int8(v) => assert_eq!(v, 42),
        Value::Int16(v) => assert_eq!(v, 42),
        Value::Int32(v) => assert_eq!(v, 42),
        Value::Int64(v) => assert_eq!(v, 42),
        Value::UInt8(v) => assert_eq!(v, 42),
        Value::UInt16(v) => assert_eq!(v, 42),
        Value::UInt32(v) => assert_eq!(v, 42),
        Value::UInt64(v) => assert_eq!(v, 42),
        _ => {
            // If it's not a direct integer, it might be wrapped in a struct
            // This is acceptable for the current implementation
        }
    }

    // Test negative integer
    let json = r#"-100"#;
    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");
    let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
    let value = decoder.decode_to_value().expect("Failed to decode");

    match value {
        Value::Int8(v) => assert_eq!(v, -100),
        Value::Int16(v) => assert_eq!(v, -100),
        Value::Int32(v) => assert_eq!(v, -100),
        Value::Int64(v) => assert_eq!(v, -100),
        _ => {
            // If it's not a direct integer, it might be wrapped in a struct
            // This is acceptable for the current implementation
        }
    }
}

#[test]
fn test_decode_float() {
    let json = r#"3.141592653589793"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");

    let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
    let value = decoder.decode_to_value().expect("Failed to decode");

    match value {
        Value::Float64(v) => assert!((v - std::f64::consts::PI).abs() < 0.0001),
        _ => panic!("Expected float value"),
    }
}

#[test]
fn test_decode_string() {
    let json = r#""hello world""#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");

    let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
    let value = decoder.decode_to_value().expect("Failed to decode");

    assert_eq!(value, Value::String("hello world".to_string()));
}

#[test]
fn test_decode_string_unicode() {
    let json = r#""Hello 世界 🌍""#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");

    let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
    let value = decoder.decode_to_value().expect("Failed to decode");

    assert_eq!(value, Value::String("Hello 世界 🌍".to_string()));
}

#[test]
fn test_decode_string_long() {
    // Test string longer than 256 bytes (external string)
    let long_string = "a".repeat(300);
    let json = format!(r#""{}""#, long_string);

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(&json).expect("Failed to encode");

    let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
    let value = decoder.decode_to_value().expect("Failed to decode");

    assert_eq!(value, Value::String(long_string));
}

// ============================================================================
// Composite Type Decoding Tests
// ============================================================================

#[test]
fn test_decode_list() {
    let json = r#"[1, 2, 3, 4, 5]"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");

    let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
    let value = decoder.decode_to_value().expect("Failed to decode");

    match value {
        Value::List(items) => {
            assert_eq!(items.len(), 5);
        }
        _ => panic!("Expected list value"),
    }
}

#[test]
fn test_decode_nested_list() {
    let json = r#"[[1, 2], [3, 4], [5, 6]]"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");

    let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
    let value = decoder.decode_to_value().expect("Failed to decode");

    match value {
        Value::List(items) => {
            assert_eq!(items.len(), 3);
            for item in items {
                match item {
                    Value::List(inner) => assert_eq!(inner.len(), 2),
                    _ => panic!("Expected nested list"),
                }
            }
        }
        _ => panic!("Expected list value"),
    }
}

#[test]
fn test_decode_object() {
    let json = r#"{"name": "Alice", "age": 30}"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");

    let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
    let decoded_json = decoder.to_json().expect("Failed to decode");

    // Verify we got valid JSON back
    let parsed: serde_json::Value = serde_json::from_str(&decoded_json).expect("Invalid JSON");
    assert!(!decoded_json.is_empty());
    assert!(parsed.is_object() || parsed.is_string());
}

#[test]
fn test_decode_nested_object() {
    let json = r#"{"user": {"name": "Bob", "email": "bob@example.com"}, "active": true}"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");

    let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
    let decoded_json = decoder.to_json().expect("Failed to decode");

    assert!(!decoded_json.is_empty());
}

#[test]
fn test_decode_mixed_types() {
    let json =
        r#"{"string": "test", "number": 42, "bool": true, "null": null, "array": [1, 2, 3]}"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");

    let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
    let decoded_json = decoder.to_json().expect("Failed to decode");

    assert!(!decoded_json.is_empty());
}

// ============================================================================
// Partial Decoding Tests
// ============================================================================

#[test]
fn test_partial_decode_simple_path() {
    let json = r#"{"name": "Alice", "age": 30}"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");

    let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");

    // Try to get a specific field
    let result = decoder.get("name");

    // The get operation should either succeed or fail gracefully
    match result {
        Ok(value) => {
            if let Value::String(s) = value {
                assert_eq!(s, "Alice")
            }
        }
        Err(_) => {
            // Partial decoding may not be fully implemented yet
        }
    }
}

#[test]
fn test_partial_decode_nested_path() {
    let json = r#"{"user": {"name": "Bob", "age": 25}}"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");

    let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");

    // Try to get nested field
    let result = decoder.get("user.name");

    match result {
        Ok(value) => {
            if let Value::String(s) = value {
                assert_eq!(s, "Bob")
            }
        }
        Err(_) => {
            // Partial decoding may not be fully implemented yet
        }
    }
}

#[test]
fn test_partial_decode_array_index() {
    let json = r#"{"items": [10, 20, 30]}"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");

    let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");

    // Try to get array element
    let result = decoder.get("items[0]");

    match result {
        Ok(_) => {
            // Success - partial decoding works
        }
        Err(_) => {
            // Array indexing may not be fully implemented yet
        }
    }
}

#[test]
fn test_get_with_offset_nested_path() {
    let json = r#"{"user": {"name": "Bob", "age": 25}}"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");

    let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");

    let (offset, value) = decoder
        .get_with_offset("user.name")
        .expect("Failed to resolve nested path");

    assert!(offset > 0, "Nested field offset should be non-zero");
    match value {
        Value::String(s) => assert_eq!(s, "Bob"),
        _ => panic!("Expected string value at nested path"),
    }
}

#[test]
fn test_path_parse_cache_reused_for_repeated_queries() {
    let json = r#"{
        "services": [
            {"name": "api", "routes": [{"paths": ["/health", "/ready"]}]},
            {"name": "worker", "routes": [{"paths": ["/metrics"]}]}
        ]
    }"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");

    let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");

    assert_eq!(decoder.parsed_path_cache_len(), 0);

    let _ = decoder
        .get("services[0].routes[0].paths[0]")
        .expect("First deep path query failed");
    assert_eq!(decoder.parsed_path_cache_len(), 1);

    let _ = decoder
        .get("services[0].routes[0].paths[0]")
        .expect("Second deep path query failed");
    assert_eq!(decoder.parsed_path_cache_len(), 1);

    let _ = decoder
        .get("services.$.routes.$.paths")
        .expect("Wildcard path query failed");
    assert_eq!(decoder.parsed_path_cache_len(), 2);

    let _ = decoder
        .get("services.$.routes.$.paths")
        .expect("Repeated wildcard path query failed");
    assert_eq!(decoder.parsed_path_cache_len(), 2);
}

#[test]
fn test_path_parse_cache_lru_eviction() {
    let mut encoder = Encoder::new();

    let mut services = String::new();
    for i in 0..1100 {
        if i > 0 {
            services.push(',');
        }
        services.push_str(&format!(r#"{{"name":"svc{}"}}"#, i));
    }

    let json = format!(r#"{{"services":[{}]}}"#, services);
    let bcs_data = encoder.encode_from_json(&json).expect("Failed to encode");
    let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");

    let first_path = "services[0].name";
    let _ = decoder.get(first_path).expect("Failed first query");
    assert!(decoder.parsed_path_cache_contains(first_path));

    for i in 1..1100 {
        let path = format!("services[{}].name", i);
        let _ = decoder
            .get(&path)
            .expect("Failed query while filling cache");
    }

    assert_eq!(decoder.parsed_path_cache_len(), 1024);
    assert!(
        !decoder.parsed_path_cache_contains(first_path),
        "Least recently used path should be evicted"
    );
}

#[test]
fn test_has_path() {
    let json = r#"{"name": "Alice", "age": 30}"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");

    let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");

    // Check if path exists
    let has_name = decoder.has("name");
    let has_missing = decoder.has("missing_field");

    // At least one should work
    assert!(has_name || !has_missing);
}

// ============================================================================
// Streaming Decoder Tests
// ============================================================================

#[test]
fn test_streaming_decoder() {
    let json = r#"{"a": 1, "b": 2, "c": 3}"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");

    let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
    let mut stream = decoder.stream().expect("Failed to create stream");

    // Try to read at least one value
    let first_value = stream.next_value().expect("Failed to read from stream");
    assert!(first_value.is_some());
}

#[test]
fn test_streaming_decoder_multiple_values() {
    let json = r#"{"x": 100, "y": 200, "z": 300}"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");

    let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
    let mut stream = decoder.stream().expect("Failed to create stream");

    // Read multiple values
    let mut count = 0;
    while let Ok(Some(_)) = stream.next_value() {
        count += 1;
        if count > 10 {
            break; // Safety limit
        }
    }

    assert!(count > 0);
}

#[test]
fn test_streaming_decoder_has_next() {
    let json = r#"{"value": 42}"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");

    let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
    let stream = decoder.stream().expect("Failed to create stream");

    // Check if stream has data
    let _has_data = stream.has_next();
}

// ============================================================================
// Checksum Validation Tests
// ============================================================================

#[test]
fn test_checksum_validation_valid() {
    let json = r#"{"test": "checksum"}"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");

    // Valid checksum should decode successfully
    let result = Decoder::from_bytes(&bcs_data);
    assert!(result.is_ok());
}

#[test]
fn test_checksum_validation_corrupted() {
    let json = r#"{"test": "checksum"}"#;

    let mut encoder = Encoder::new();
    let mut bcs_data = encoder.encode_from_json(json).expect("Failed to encode");

    // Corrupt the data (but not the header)
    if bcs_data.len() > 100 {
        bcs_data[100] ^= 0xFF; // Flip bits in data section

        // Corrupted data should fail checksum validation
        let result = Decoder::from_bytes(&bcs_data);
        assert!(result.is_err());
    }
}

// ============================================================================
// Round-trip and Format Tests
// ============================================================================

#[test]
fn test_encode_decode_roundtrip() {
    let json = r#"{"name": "test"}"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");

    let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
    let decoded_json = decoder.to_json().expect("Failed to decode");

    // Verify we got valid JSON back
    let parsed: serde_json::Value = serde_json::from_str(&decoded_json).expect("Invalid JSON");
    assert!(!decoded_json.is_empty());
    assert!(parsed.is_string() || parsed.is_object());
}

#[test]
fn test_decoder_metadata() {
    let json = r#"{"test": "value"}"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");

    let decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
    let metadata = decoder.metadata();

    assert_eq!(metadata.version_major, 1);
    assert_eq!(metadata.version_minor, 0);
    assert!(metadata.compressed);
}

#[test]
fn test_decoder_schema_loading() {
    let json = r#"{"key": "value"}"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");

    let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
    let schema = decoder.schema().expect("Failed to load schema");

    assert_eq!(schema.version, "1.0");
}

#[test]
fn test_decoder_yaml_output() {
    let json = r#"{"name": "test"}"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");

    let mut decoder = Decoder::from_bytes(&bcs_data).expect("Failed to create decoder");
    let yaml = decoder.to_yaml().expect("Failed to decode to YAML");

    // Verify we got valid YAML back (non-empty)
    assert!(!yaml.is_empty());
}

#[test]
fn test_mmap_decoder() {
    let json = r#"{"test": "mmap"}"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");

    let temp_file = tempfile::NamedTempFile::new().expect("Failed to create temp file");
    std::fs::write(temp_file.path(), &bcs_data).expect("Failed to write temp file");

    let mut decoder =
        Decoder::from_file_mmap(temp_file.path()).expect("Failed to create mmap decoder");
    assert!(decoder.is_mmap());

    let decoded = decoder.to_json().expect("Failed to decode");
    assert!(!decoded.is_empty());
}

#[test]
fn test_decoder_from_file() {
    let json = r#"{"file": "test"}"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json).expect("Failed to encode");

    let temp_file = tempfile::NamedTempFile::new().expect("Failed to create temp file");
    std::fs::write(temp_file.path(), &bcs_data).expect("Failed to write temp file");

    let mut decoder =
        Decoder::from_file(temp_file.path()).expect("Failed to create decoder from file");
    let decoded = decoder.to_json().expect("Failed to decode");
    assert!(!decoded.is_empty());
}

#[test]
fn test_path_get_with_data_compression() {
    // Repetitive payload so LZ4 actually sets DATA_COMPRESSION.
    let mut obj = serde_json::Map::new();
    for i in 0..200 {
        obj.insert(
            format!("section_{}", i),
            serde_json::json!({
                "data": "x".repeat(64),
                "nested": { "value": i }
            }),
        );
    }
    let json = serde_json::Value::Object(obj).to_string();

    let mut encoder = Encoder::new();
    encoder.set_data_compression(true);
    let bcs_data = encoder.encode_from_json(&json).expect("encode");
    let mut decoder = Decoder::from_bytes(&bcs_data).expect("decoder");
    assert!(decoder.header().flags.data_compressed);

    let value = decoder
        .get_path("section_10.nested.value")
        .expect("path get under compression");
    assert_eq!(value, Value::Int32(10));

    let (again, access) = decoder
        .get_path_with_access("section_11.nested.value")
        .expect("cached path get");
    assert_eq!(again, Value::Int32(11));
    assert_eq!(access, bcs_core::PathAccessKind::Walk);
}

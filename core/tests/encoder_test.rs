use bcs_core::{Encoder, Result};

// ============================================================================
// Primitive Type Encoding Tests
// ============================================================================

#[test]
fn test_encode_null() -> Result<()> {
    let json = r#"{"value": null}"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json)?;

    assert!(!bcs_data.is_empty());
    assert_eq!(&bcs_data[0..4], &[0x46, 0x53, 0x43, 0x42]);

    Ok(())
}

#[test]
fn test_encode_boolean() -> Result<()> {
    let json = r#"{"flag_true": true, "flag_false": false}"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json)?;

    assert!(!bcs_data.is_empty());
    assert_eq!(&bcs_data[0..4], &[0x46, 0x53, 0x43, 0x42]);

    Ok(())
}

#[test]
fn test_encode_integers() -> Result<()> {
    let json = r#"{
        "small": 42,
        "large": 2147483647,
        "negative": -100
    }"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json)?;

    assert!(!bcs_data.is_empty());
    assert!(bcs_data.len() > 72);

    Ok(())
}

#[test]
fn test_encode_unsigned_integers() -> Result<()> {
    let json = r#"{
        "small": 255,
        "medium": 65535,
        "large": 4294967295
    }"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json)?;

    assert!(!bcs_data.is_empty());

    Ok(())
}

#[test]
fn test_encode_floats() -> Result<()> {
    let json = r#"{
        "pi": 3.14159,
        "e": 2.71828,
        "negative": -1.5
    }"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json)?;

    assert!(!bcs_data.is_empty());

    Ok(())
}

#[test]
fn test_encode_strings() -> Result<()> {
    let json = r#"{
        "short": "hello",
        "empty": "",
        "unicode": "Hello 世界 🌍",
        "special": "Line1\nLine2\tTabbed"
    }"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json)?;

    assert!(!bcs_data.is_empty());

    Ok(())
}

#[test]
fn test_encode_long_string() -> Result<()> {
    // Test string longer than 256 bytes (triggers external string encoding)
    let long_string = "a".repeat(300);
    let json = format!(r#"{{"long_text": "{}"}}"#, long_string);

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(&json)?;

    assert!(!bcs_data.is_empty());

    Ok(())
}

// ============================================================================
// Composite Type Encoding Tests
// ============================================================================

#[test]
fn test_encode_array() -> Result<()> {
    let json = r#"{
        "items": [1, 2, 3, 4, 5]
    }"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json)?;

    assert!(!bcs_data.is_empty());

    Ok(())
}

#[test]
fn test_encode_empty_array() -> Result<()> {
    let json = r#"{"empty": []}"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json)?;

    assert!(!bcs_data.is_empty());

    Ok(())
}

#[test]
fn test_encode_nested_arrays() -> Result<()> {
    let json = r#"{
        "matrix": [[1, 2, 3], [4, 5, 6], [7, 8, 9]]
    }"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json)?;

    assert!(!bcs_data.is_empty());

    Ok(())
}

#[test]
fn test_encode_mixed_array() -> Result<()> {
    let json = r#"{
        "mixed": [1, "two", 3.0, true, null]
    }"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json)?;

    assert!(!bcs_data.is_empty());

    Ok(())
}

#[test]
fn test_encode_nested_objects() -> Result<()> {
    let json = r#"{
        "server": {
            "host": "localhost",
            "port": 8080,
            "ssl": {
                "enabled": true,
                "cert": "/path/to/cert"
            }
        }
    }"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json)?;

    assert!(!bcs_data.is_empty());
    assert!(bcs_data.len() > 72);

    Ok(())
}

#[test]
fn test_encode_deeply_nested_structure() -> Result<()> {
    let json = r#"{
        "level1": {
            "level2": {
                "level3": {
                    "level4": {
                        "value": "deep"
                    }
                }
            }
        }
    }"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json)?;

    assert!(!bcs_data.is_empty());

    Ok(())
}

#[test]
fn test_encode_complex_structure() -> Result<()> {
    let json = r#"{
        "users": [
            {
                "id": 1,
                "name": "Alice",
                "email": "alice@example.com",
                "active": true
            },
            {
                "id": 2,
                "name": "Bob",
                "email": "bob@example.com",
                "active": false
            }
        ],
        "metadata": {
            "version": "1.0",
            "timestamp": 1234567890
        }
    }"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json)?;

    assert!(!bcs_data.is_empty());

    Ok(())
}

// ============================================================================
// JSON/YAML/TOML Input Parsing Tests
// ============================================================================

#[test]
fn test_encode_simple_json() -> Result<()> {
    let json = r#"{"name": "test", "value": 42}"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json)?;

    assert!(!bcs_data.is_empty());
    assert_eq!(&bcs_data[0..4], &[0x46, 0x53, 0x43, 0x42]);

    Ok(())
}

#[test]
fn test_encode_simple_yaml() -> Result<()> {
    let yaml = r#"
name: test
value: 42
"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_yaml(yaml)?;

    assert!(!bcs_data.is_empty());
    assert_eq!(&bcs_data[0..4], &[0x46, 0x53, 0x43, 0x42]);

    Ok(())
}

#[test]
fn test_encode_simple_toml() -> Result<()> {
    let toml = r#"
name = "test"
value = 42
"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_toml(toml)?;

    assert!(!bcs_data.is_empty());
    assert_eq!(&bcs_data[0..4], &[0x46, 0x53, 0x43, 0x42]);

    Ok(())
}

#[test]
fn test_encode_yaml_with_arrays() -> Result<()> {
    let yaml = r#"
items:
  - name: item1
    value: 100
  - name: item2
    value: 200
"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_yaml(yaml)?;

    assert!(!bcs_data.is_empty());

    Ok(())
}

#[test]
fn test_encode_toml_with_tables() -> Result<()> {
    let toml = r#"
[server]
host = "localhost"
port = 8080

[database]
name = "mydb"
connections = 10
"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_toml(toml)?;

    assert!(!bcs_data.is_empty());

    Ok(())
}

#[test]
fn test_encode_yaml_multiline_string() -> Result<()> {
    let yaml = r#"
description: |
  This is a multiline
  string in YAML
  with multiple lines
"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_yaml(yaml)?;

    assert!(!bcs_data.is_empty());

    Ok(())
}

#[test]
fn test_encode_invalid_json() {
    let invalid_json = r#"{"name": "test", invalid}"#;

    let mut encoder = Encoder::new();
    let result = encoder.encode_from_json(invalid_json);

    assert!(result.is_err());
}

#[test]
fn test_encode_invalid_yaml() {
    // Use truly malformed YAML with invalid syntax
    let invalid_yaml = "{ invalid: yaml: syntax";

    let mut encoder = Encoder::new();
    let result = encoder.encode_from_yaml(invalid_yaml);

    assert!(result.is_err());
}

#[test]
fn test_encode_invalid_toml() {
    let invalid_toml = r#"
name = "test
value = 42
"#;

    let mut encoder = Encoder::new();
    let result = encoder.encode_from_toml(invalid_toml);

    assert!(result.is_err());
}

// ============================================================================
// Checksum Calculation Tests
// ============================================================================

#[test]
fn test_checksum_verification() -> Result<()> {
    let json = r#"{"name": "test", "value": 42}"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json)?;

    let is_valid = Encoder::verify_checksum(&bcs_data)?;
    assert!(is_valid, "Checksum verification failed");

    Ok(())
}

#[test]
fn test_checksum_with_large_data() -> Result<()> {
    let json = r#"{
        "data": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
        "text": "This is a longer text to test checksum calculation",
        "nested": {
            "field1": "value1",
            "field2": "value2",
            "field3": "value3"
        }
    }"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json)?;

    let is_valid = Encoder::verify_checksum(&bcs_data)?;
    assert!(is_valid);

    Ok(())
}

#[test]
fn test_checksum_detects_corruption() -> Result<()> {
    let json = r#"{"name": "test", "value": 42}"#;

    let mut encoder = Encoder::new();
    let mut bcs_data = encoder.encode_from_json(json)?;

    // Corrupt a byte in the data section (after header)
    if bcs_data.len() > 100 {
        bcs_data[100] ^= 0xFF;
    }

    let is_valid = Encoder::verify_checksum(&bcs_data)?;
    assert!(!is_valid, "Checksum should detect corruption");

    Ok(())
}

#[test]
fn test_checksum_different_data() -> Result<()> {
    let json1 = r#"{"name": "test1", "value": 42}"#;
    let json2 = r#"{"name": "test2", "value": 43}"#;

    let mut encoder1 = Encoder::new();
    let bcs_data1 = encoder1.encode_from_json(json1)?;

    let mut encoder2 = Encoder::new();
    let bcs_data2 = encoder2.encode_from_json(json2)?;

    // Extract checksums
    let checksum1 = u64::from_le_bytes(bcs_data1[56..64].try_into().unwrap());
    let checksum2 = u64::from_le_bytes(bcs_data2[56..64].try_into().unwrap());

    // Different input should produce different checksums
    assert_ne!(checksum1, checksum2);

    Ok(())
}

// ============================================================================
// Configuration and Flag Tests
// ============================================================================

#[test]
fn test_encode_with_compression() -> Result<()> {
    let json = r#"{"name": "test", "value": 42}"#;

    let mut encoder = Encoder::new();
    encoder.set_compression(true);
    let bcs_data = encoder.encode_from_json(json)?;

    let flags = u16::from_le_bytes([bcs_data[6], bcs_data[7]]);
    assert_eq!(flags & 0x0001, 0x0001);

    Ok(())
}

#[test]
fn test_encode_without_compression() -> Result<()> {
    let json = r#"{"name": "test", "value": 42}"#;

    let mut encoder = Encoder::new();
    encoder.set_compression(false);
    let bcs_data = encoder.encode_from_json(json)?;

    let flags = u16::from_le_bytes([bcs_data[6], bcs_data[7]]);
    assert_eq!(flags & 0x0001, 0x0000);

    Ok(())
}

#[test]
fn test_reserved_header_bit_is_never_set_on_encode() -> Result<()> {
    let json = r#"{"name": "test", "value": 42}"#;

    let mut encoder = Encoder::new();
    #[allow(deprecated)]
    encoder.set_ai_metadata(true);
    let bcs_data = encoder.encode_from_json(json)?;

    let flags = u16::from_le_bytes([bcs_data[6], bcs_data[7]]);
    assert_eq!(
        flags & 0x0002,
        0,
        "reserved bit 0x0002 must be clear on newly encoded files"
    );

    Ok(())
}

#[test]
fn test_encode_with_data_compression_flag_set_when_beneficial() -> Result<()> {
    // Repetitive payload should compress well.
    let json = r#"{"blob":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}"#;

    let mut encoder = Encoder::new();
    encoder.set_data_compression(true);
    let bcs_data = encoder.encode_from_json(json)?;

    let flags = u16::from_le_bytes([bcs_data[6], bcs_data[7]]);
    assert_eq!(flags & 0x0004, 0x0004);

    Ok(())
}

#[test]
fn test_encode_with_data_compression_flag_unset_when_not_beneficial() -> Result<()> {
    // Tiny payload should not gain from LZ4; smart compression keeps raw data.
    let json = r#"{"a":1}"#;

    let mut encoder = Encoder::new();
    encoder.set_data_compression(true);
    let bcs_data = encoder.encode_from_json(json)?;

    let flags = u16::from_le_bytes([bcs_data[6], bcs_data[7]]);
    assert_eq!(flags & 0x0004, 0x0000);

    Ok(())
}

// ============================================================================
// Header Validation Tests
// ============================================================================

#[test]
fn test_encode_header_magic_number() -> Result<()> {
    let json = r#"{"test": true}"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json)?;

    let magic = u32::from_le_bytes([bcs_data[0], bcs_data[1], bcs_data[2], bcs_data[3]]);
    assert_eq!(magic, 0x42435346);

    Ok(())
}

#[test]
fn test_encode_header_version() -> Result<()> {
    let json = r#"{"test": true}"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json)?;

    assert_eq!(bcs_data[4], 1); // Major version
    assert_eq!(bcs_data[5], 0); // Minor version

    Ok(())
}

#[test]
fn test_encode_header_offsets() -> Result<()> {
    let json = r#"{"name": "test", "value": 42}"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json)?;

    // Semantic offset should be 64 (header size)
    let semantic_offset = u64::from_le_bytes(bcs_data[8..16].try_into().unwrap());
    assert_eq!(semantic_offset, 64);

    // All offsets should be valid
    let semantic_size = u64::from_le_bytes(bcs_data[16..24].try_into().unwrap());
    let index_offset = u64::from_le_bytes(bcs_data[24..32].try_into().unwrap());
    let index_size = u64::from_le_bytes(bcs_data[32..40].try_into().unwrap());
    let data_offset = u64::from_le_bytes(bcs_data[40..48].try_into().unwrap());
    let data_size = u64::from_le_bytes(bcs_data[48..56].try_into().unwrap());

    assert!(semantic_size > 0);
    assert_eq!(index_offset, semantic_offset + semantic_size);
    assert!(index_size > 0);
    assert_eq!(data_offset, index_offset + index_size);
    assert!(data_size > 0);

    Ok(())
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[test]
fn test_encode_empty_object() -> Result<()> {
    let json = r#"{}"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json)?;

    assert!(!bcs_data.is_empty());
    assert_eq!(&bcs_data[0..4], &[0x46, 0x53, 0x43, 0x42]);

    Ok(())
}

#[test]
fn test_encode_large_numbers() -> Result<()> {
    let json = r#"{
        "max_i32": 2147483647,
        "min_i32": -2147483648,
        "large_u64": 18446744073709551615
    }"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json)?;

    assert!(!bcs_data.is_empty());

    Ok(())
}

#[test]
fn test_encode_special_float_values() -> Result<()> {
    let json = r#"{
        "zero": 0.0,
        "negative_zero": -0.0,
        "small": 0.000001,
        "large": 1e10
    }"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json)?;

    assert!(!bcs_data.is_empty());

    Ok(())
}

#[test]
fn test_encode_unicode_strings() -> Result<()> {
    let json = r#"{
        "chinese": "你好世界",
        "arabic": "مرحبا بالعالم",
        "emoji": "🎉🎊🎈",
        "mixed": "Hello 世界 🌍"
    }"#;

    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json)?;

    assert!(!bcs_data.is_empty());

    Ok(())
}

#[test]
fn test_encode_reset() -> Result<()> {
    let json1 = r#"{"first": 1}"#;
    let json2 = r#"{"second": 2}"#;

    let mut encoder = Encoder::new();

    let bcs_data1 = encoder.encode_from_json(json1)?;
    assert!(!bcs_data1.is_empty());

    let bcs_data2 = encoder.encode_from_json(json2)?;
    assert!(!bcs_data2.is_empty());

    // Both should be valid
    assert!(Encoder::verify_checksum(&bcs_data1)?);
    assert!(Encoder::verify_checksum(&bcs_data2)?);

    Ok(())
}

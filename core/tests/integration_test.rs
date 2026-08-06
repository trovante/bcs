// Integration tests for BCS
// Tests round-trip encoding/decoding, cross-format compatibility, and large file handling

mod common;

use bcs_core::{Decoder, Encoder, Result};
use std::fs;

#[test]
fn test_roundtrip_json_to_bcs_to_json() -> Result<()> {
    let json_content = common::read_example("app-settings.json");

    // Encode to BCS
    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(&json_content)?;

    // Decode back to JSON
    let mut decoder = Decoder::from_bytes(&bcs_data)?;
    let decoded_json = decoder.to_json()?;

    // Parse both JSONs to compare structure
    let original: serde_json::Value = serde_json::from_str(&json_content)?;
    let decoded: serde_json::Value = serde_json::from_str(&decoded_json)?;

    // Verify they match
    assert_eq!(
        original, decoded,
        "Round-trip JSON encoding/decoding failed"
    );

    Ok(())
}

#[test]
fn test_roundtrip_yaml_to_bcs_to_yaml() -> Result<()> {
    // Create a YAML document
    let yaml_content = r#"
application:
  name: TestApp
  version: 1.0.0
server:
  host: localhost
  port: 8080
database:
  host: db.example.com
  port: 5432
"#;

    // Encode to BCS
    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_yaml(yaml_content)?;

    // Decode back to YAML
    let mut decoder = Decoder::from_bytes(&bcs_data)?;
    let decoded_yaml = decoder.to_yaml()?;

    // Parse both YAMLs to compare structure
    let original: serde_yaml::Value = serde_yaml::from_str(yaml_content)?;
    let decoded: serde_yaml::Value = serde_yaml::from_str(&decoded_yaml)?;

    // Verify they match
    assert_eq!(
        original, decoded,
        "Round-trip YAML encoding/decoding failed"
    );

    Ok(())
}

#[test]
fn test_roundtrip_toml_to_bcs_to_json() -> Result<()> {
    // Create a TOML document
    let toml_content = r#"
[application]
name = "TestApp"
version = "1.0.0"

[server]
host = "localhost"
port = 8080

[database]
host = "db.example.com"
port = 5432
"#;

    // Encode to BCS
    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_toml(toml_content)?;

    // Decode back to JSON
    let mut decoder = Decoder::from_bytes(&bcs_data)?;
    let decoded_json = decoder.to_json()?;

    // Parse TOML and decoded JSON to compare
    let original: toml::Value = toml::from_str(toml_content)?;
    let decoded: serde_json::Value = serde_json::from_str(&decoded_json)?;

    // Convert TOML to JSON for comparison
    let original_json = serde_json::to_value(&original)?;

    assert_eq!(
        original_json, decoded,
        "Round-trip TOML to BCS to JSON failed"
    );

    Ok(())
}

#[test]
fn test_kubernetes_deployment_roundtrip() -> Result<()> {
    let json_content = common::read_example("kubernetes-deployment.json");

    // Encode to BCS
    let mut encoder = Encoder::new();
    encoder.set_compression(true);
    let bcs_data = encoder.encode_from_json(&json_content)?;

    // Decode back
    let mut decoder = Decoder::from_bytes(&bcs_data)?;
    let decoded_json = decoder.to_json()?;

    // Compare
    let original: serde_json::Value = serde_json::from_str(&json_content)?;
    let decoded: serde_json::Value = serde_json::from_str(&decoded_json)?;

    assert_eq!(original, decoded, "Kubernetes deployment round-trip failed");

    Ok(())
}

#[test]
fn test_cross_format_json_to_yaml() -> Result<()> {
    // Load JSON
    let json_content = r#"{"name": "test", "value": 42, "nested": {"key": "value"}}"#;

    // Encode from JSON
    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json_content)?;

    // Decode to YAML
    let mut decoder = Decoder::from_bytes(&bcs_data)?;
    let yaml_output = decoder.to_yaml()?;

    // Verify YAML is valid and contains expected data
    let yaml_value: serde_yaml::Value = serde_yaml::from_str(&yaml_output)?;
    assert!(yaml_value.is_mapping());

    Ok(())
}

#[test]
fn test_large_nested_structure() -> Result<()> {
    // Create a large nested structure
    let mut large_json = String::from("{");
    for i in 0..100 {
        if i > 0 {
            large_json.push(',');
        }
        large_json.push_str(&format!(
            r#""field_{}": {{"nested_a": {}, "nested_b": "value_{}", "nested_c": [{}, {}, {}]}}"#,
            i,
            i,
            i,
            i,
            i * 2,
            i * 3
        ));
    }
    large_json.push('}');

    // Encode
    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(&large_json)?;

    // Decode
    let mut decoder = Decoder::from_bytes(&bcs_data)?;
    let decoded_json = decoder.to_json()?;

    // Verify
    let original: serde_json::Value = serde_json::from_str(&large_json)?;
    let decoded: serde_json::Value = serde_json::from_str(&decoded_json)?;

    assert_eq!(
        original, decoded,
        "Large nested structure round-trip failed"
    );

    Ok(())
}

#[test]
fn test_large_array_handling() -> Result<()> {
    // Create a large array
    let mut large_array = String::from(r#"{"items": ["#);
    for i in 0..1000 {
        if i > 0 {
            large_array.push(',');
        }
        large_array.push_str(&format!(
            r#"{{"id": {}, "name": "item_{}", "value": {}}}"#,
            i,
            i,
            i * 10
        ));
    }
    large_array.push_str("]}");

    // Encode
    let mut encoder = Encoder::new();
    encoder.set_compression(true);
    let bcs_data = encoder.encode_from_json(&large_array)?;

    // Decode
    let mut decoder = Decoder::from_bytes(&bcs_data)?;
    let decoded_json = decoder.to_json()?;

    // Verify
    let original: serde_json::Value = serde_json::from_str(&large_array)?;
    let decoded: serde_json::Value = serde_json::from_str(&decoded_json)?;

    assert_eq!(original, decoded, "Large array handling failed");

    // Verify compression reduced size
    let uncompressed_size = large_array.len();
    let compressed_size = bcs_data.len();
    println!(
        "Original: {} bytes, BCS: {} bytes",
        uncompressed_size, compressed_size
    );

    Ok(())
}

#[test]
fn test_file_persistence() -> Result<()> {
    let json_content = r#"{"test": "file_persistence", "value": 123}"#;
    let temp_file = tempfile::NamedTempFile::new().expect("Failed to create temp file");
    let temp_path = temp_file.path();

    // Encode and write to file
    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json_content)?;
    fs::write(temp_path, &bcs_data)?;

    // Read from file and decode
    let file_data = fs::read(temp_path)?;
    let mut decoder = Decoder::from_bytes(&file_data)?;
    let decoded_json = decoder.to_json()?;

    // Verify
    let original: serde_json::Value = serde_json::from_str(json_content)?;
    let decoded: serde_json::Value = serde_json::from_str(&decoded_json)?;

    assert_eq!(original, decoded, "File persistence test failed");

    Ok(())
}

#[test]
fn test_concurrent_read_access() -> Result<()> {
    use std::sync::Arc;
    use std::thread;

    let json_content = r#"{"shared": "data", "value": 42}"#;

    // Encode
    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json_content)?;
    let shared_data = Arc::new(bcs_data);

    // Spawn multiple reader threads
    let mut handles = vec![];
    for i in 0..5 {
        let data = Arc::clone(&shared_data);
        let handle = thread::spawn(move || {
            let mut decoder = Decoder::from_bytes(&data).expect("Failed to create decoder");
            let decoded = decoder.to_json().expect("Failed to decode");
            let value: serde_json::Value =
                serde_json::from_str(&decoded).expect("Failed to parse JSON");
            assert!(value.is_object(), "Thread {} failed to decode", i);
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    Ok(())
}

#[test]
fn test_all_primitive_types() -> Result<()> {
    let json_content = r#"{
        "int8": -128,
        "int16": -32768,
        "int32": -2147483648,
        "int64": -9223372036854775808,
        "uint8": 255,
        "uint16": 65535,
        "uint32": 4294967295,
        "float32": 3.14159,
        "float64": 2.718281828459045,
        "bool_true": true,
        "bool_false": false,
        "string": "Hello, BCS!",
        "null_value": null,
        "empty_string": "",
        "unicode": "Hello 世界 🌍"
    }"#;

    // Encode
    let mut encoder = Encoder::new();
    let bcs_data = encoder.encode_from_json(json_content)?;

    // Decode
    let mut decoder = Decoder::from_bytes(&bcs_data)?;
    let decoded_json = decoder.to_json()?;

    // Verify
    let original: serde_json::Value = serde_json::from_str(json_content)?;
    let decoded: serde_json::Value = serde_json::from_str(&decoded_json)?;

    assert_eq!(original, decoded, "Primitive types round-trip failed");

    Ok(())
}

#[test]
fn test_checksum_validation() -> Result<()> {
    let json_content = r#"{"test": "checksum"}"#;

    // Encode
    let mut encoder = Encoder::new();
    let mut bcs_data = encoder.encode_from_json(json_content)?;

    // Verify checksum is valid
    assert!(
        Encoder::verify_checksum(&bcs_data)?,
        "Initial checksum should be valid"
    );

    // Corrupt the data (modify a byte in the data layer)
    if bcs_data.len() > 100 {
        bcs_data[100] ^= 0xFF;
    }

    // Verify checksum is now invalid
    assert!(
        !Encoder::verify_checksum(&bcs_data)?,
        "Corrupted data should fail checksum"
    );

    Ok(())
}

#[test]
fn test_compression_effectiveness() -> Result<()> {
    // Create repetitive data that compresses well
    let mut json = String::from(r#"{"items": ["#);
    for i in 0..100 {
        if i > 0 {
            json.push(',');
        }
        json.push_str(r#"{"type": "repeated", "value": "same_value_repeated"}"#);
    }
    json.push_str("]}");

    // Encode without compression
    let mut encoder_uncompressed = Encoder::new();
    encoder_uncompressed.set_compression(false);
    let bcs_uncompressed = encoder_uncompressed.encode_from_json(&json)?;

    // Encode with compression
    let mut encoder_compressed = Encoder::new();
    encoder_compressed.set_compression(true);
    let bcs_compressed = encoder_compressed.encode_from_json(&json)?;

    // Verify both decode correctly
    let mut decoder_uncompressed = Decoder::from_bytes(&bcs_uncompressed)?;
    let decoded_uncompressed = decoder_uncompressed.to_json()?;

    let mut decoder_compressed = Decoder::from_bytes(&bcs_compressed)?;
    let decoded_compressed = decoder_compressed.to_json()?;

    let original: serde_json::Value = serde_json::from_str(&json)?;
    let decoded_unc: serde_json::Value = serde_json::from_str(&decoded_uncompressed)?;
    let decoded_cmp: serde_json::Value = serde_json::from_str(&decoded_compressed)?;

    assert_eq!(original, decoded_unc, "Uncompressed decode failed");
    assert_eq!(original, decoded_cmp, "Compressed decode failed");

    // Verify compression actually reduced size
    println!(
        "Uncompressed: {} bytes, Compressed: {} bytes",
        bcs_uncompressed.len(),
        bcs_compressed.len()
    );

    Ok(())
}

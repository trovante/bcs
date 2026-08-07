//! Structural dedup + nested map index tests

use bcs_core::string_table::{DedupMode, DedupThresholds};
use bcs_core::{Decoder, Encoder, EncoderConfig};

#[test]
fn dedup_strings_round_trip_and_smaller() {
    let mut items = Vec::new();
    for i in 0..50 {
        items.push(format!(
            r#"{{"id":{},"role":"service-account-role","region":"us-east-1"}}"#,
            i
        ));
    }
    let json = format!("[{}]", items.join(","));

    let mut plain = Encoder::new();
    let plain_bytes = plain.encode_from_json(&json).expect("plain encode");

    let mut dedup = Encoder::with_config(EncoderConfig {
        dedup: DedupMode::Strings,
        dedup_thresholds: DedupThresholds {
            min_repeats: 2,
            min_length: 4,
        },
        ..EncoderConfig::default()
    });
    let dedup_bytes = dedup.encode_from_json(&json).expect("dedup encode");

    assert!(
        dedup_bytes.len() < plain_bytes.len(),
        "dedup {} should be smaller than plain {}",
        dedup_bytes.len(),
        plain_bytes.len()
    );

    let mut decoder = Decoder::from_bytes(&dedup_bytes).expect("decoder");
    assert!(decoder.header().flags.structural_dedup);
    let decoded = decoder.to_json().expect("decode");
    let original: serde_json::Value = serde_json::from_str(&json).unwrap();
    let round: serde_json::Value = serde_json::from_str(&decoded).unwrap();
    assert_eq!(original, round);
}

#[test]
fn dedup_keys_and_path_get() {
    let mut fields = serde_json::Map::new();
    for i in 0..30 {
        fields.insert(
            format!("svc_{}", i),
            serde_json::json!({
                "tier": "production",
                "owner": "platform-team"
            }),
        );
    }
    let json = serde_json::Value::Object(fields).to_string();

    let mut encoder = Encoder::with_config(EncoderConfig {
        dedup: DedupMode::All,
        dedup_thresholds: DedupThresholds {
            min_repeats: 2,
            min_length: 3,
        },
        index_maps_over: Some(10),
        ..EncoderConfig::default()
    });
    let bcs = encoder.encode_from_json(&json).expect("encode");
    let mut decoder = Decoder::from_bytes(&bcs).expect("decoder");
    assert!(decoder.header().flags.structural_dedup);

    let value = decoder.get("svc_5.tier").expect("path get");
    assert_eq!(value, bcs_core::types::Value::String("production".into()));
}

#[test]
fn index_maps_over_registers_nested_paths() {
    let mut fields = serde_json::Map::new();
    for i in 0..20 {
        fields.insert(format!("f{}", i), serde_json::json!({"v": i}));
    }
    let json = serde_json::json!({ "big": fields }).to_string();

    let mut encoder = Encoder::with_config(EncoderConfig {
        index_maps_over: Some(10),
        ..EncoderConfig::default()
    });
    let bcs = encoder.encode_from_json(&json).expect("encode");
    let mut decoder = Decoder::from_bytes(&bcs).expect("decoder");
    let value = decoder.get("big.f5.v").expect("nested indexed path");
    assert_eq!(value, bcs_core::types::Value::Int32(5));
}

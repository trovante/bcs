//! Adversarial / malformed-input smoke tests.
//! These are not a full fuzzer, but catch obvious panics and OOM-style length abuse.

use bcs_core::limits::{MAX_COLLECTION_LEN, MAX_INDEX_BUCKETS, MAX_NESTING_DEPTH};
use bcs_core::types::{Header, Value, HEADER_SIZE, MAGIC_NUMBER, VERSION_MAJOR, VERSION_MINOR};
use bcs_core::{Decoder, Encoder};

#[test]
fn huge_collection_length_is_rejected() {
    // List tag + absurd u32 length should fail before allocating max memory.
    let mut payload = vec![0x40]; // TypeTag::List
    payload.extend_from_slice(&(MAX_COLLECTION_LEN as u32 + 1).to_le_bytes());
    let err = bcs_core::types::CompositeDecoder::new()
        .decode_value(&mut std::io::Cursor::new(&payload))
        .unwrap_err();
    assert!(err.to_string().contains("exceeds limit"));
}

#[test]
fn huge_index_bucket_count_is_rejected() {
    let mut buf = Vec::new();
    buf.extend_from_slice(&1u32.to_le_bytes()); // entry_count
    buf.extend_from_slice(&((MAX_INDEX_BUCKETS as u32) + 1).to_le_bytes());
    buf.extend_from_slice(&0.75f32.to_le_bytes());

    let err = match bcs_core::index::IndexTable::read(&mut std::io::Cursor::new(&buf)) {
        Ok(_) => panic!("expected index bucket limit error"),
        Err(e) => e,
    };
    assert!(err.to_string().contains("exceeds limit"));
}

#[test]
fn section_offset_overflow_is_rejected() {
    let mut data = vec![0u8; HEADER_SIZE];
    // magic
    data[0..4].copy_from_slice(&MAGIC_NUMBER.to_le_bytes());
    data[4] = VERSION_MAJOR;
    data[5] = VERSION_MINOR;
    // data_offset = usize::MAX-ish as u64, data_size = 100 → overflow/out of bounds
    data[40..48].copy_from_slice(&(u64::MAX - 10).to_le_bytes());
    data[48..56].copy_from_slice(&100u64.to_le_bytes());

    // Patch checksum so header parse path reaches section checks on decode.
    let mut header = Header::read(&mut std::io::Cursor::new(&data)).unwrap();
    // Recalculate checksum excluding checksum field.
    let mut to_check = Vec::new();
    to_check.extend_from_slice(&data[0..56]);
    to_check.extend_from_slice(&data[64..]);
    header.checksum = Encoder::calculate_crc64(&to_check);
    data[56..64].copy_from_slice(&header.checksum.to_le_bytes());

    let mut decoder = Decoder::from_bytes(&data).unwrap();
    let err = decoder.decode_to_value().unwrap_err();
    assert!(
        err.to_string().contains("overflow")
            || err.to_string().contains("out of bounds")
            || err.to_string().contains("address space")
    );
}

#[test]
fn random_short_buffers_do_not_panic() {
    for seed in 0u64..64 {
        let mut buf = vec![0u8; (seed as usize % 80) + 1];
        for (i, b) in buf.iter_mut().enumerate() {
            *b = ((seed.wrapping_mul(1103515245).wrapping_add(i as u64)) & 0xff) as u8;
        }
        let _ = Decoder::from_bytes(&buf);
    }
}

#[test]
fn mmap_decoder_sets_is_mmap_flag() {
    let encoded = Encoder::new()
        .encode_from_json(r#"{"a":1,"b":"x"}"#)
        .unwrap();
    let path = std::env::temp_dir().join(format!("bcs_mmap_{}.bcs", std::process::id()));
    std::fs::write(&path, &encoded).unwrap();
    let decoder = Decoder::from_file_mmap(&path).unwrap();
    assert!(decoder.is_mmap());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn excessive_nesting_depth_is_rejected_on_decode() {
    // Nested lists: [[[[[...]]]]] with depth > MAX_NESTING_DEPTH
    let mut payload = Vec::new();
    for _ in 0..=(MAX_NESTING_DEPTH + 1) {
        payload.push(0x40); // TypeTag::List
        payload.extend_from_slice(&1u32.to_le_bytes()); // one element
    }
    payload.push(0x00); // Null leaf

    let err = bcs_core::types::CompositeDecoder::new()
        .decode_value(&mut std::io::Cursor::new(&payload))
        .unwrap_err();
    assert!(
        err.to_string().contains("Nesting depth"),
        "unexpected error: {err}"
    );
}

#[test]
fn excessive_nesting_depth_is_rejected_on_encode() {
    let mut value = Value::Null;
    for _ in 0..=(MAX_NESTING_DEPTH + 1) {
        value = Value::List(vec![value]);
    }

    let err = bcs_core::types::CompositeEncoder::new()
        .encode_value(&mut Vec::new(), &value)
        .unwrap_err();
    assert!(
        err.to_string().contains("Nesting depth"),
        "unexpected error: {err}"
    );
}

#[test]
fn nesting_at_limit_is_accepted() {
    let mut value = Value::Int32(1);
    for _ in 0..MAX_NESTING_DEPTH {
        value = Value::List(vec![value]);
    }

    let mut buf = Vec::new();
    bcs_core::types::CompositeEncoder::new()
        .encode_value(&mut buf, &value)
        .expect("encode at max depth should succeed");
    let decoded = bcs_core::types::CompositeDecoder::new()
        .decode_value(&mut std::io::Cursor::new(&buf))
        .expect("decode at max depth should succeed");
    assert!(matches!(decoded, Value::List(_)));
}

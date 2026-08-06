//! Resource limits for untrusted BCS input decoding.
//!
//! These caps prevent unbounded allocations from crafted length fields and
//! section sizes before the payload has been validated.

use crate::error::{BCSError, Result};

/// Maximum length for lists, maps, and struct field counts.
pub const MAX_COLLECTION_LEN: usize = 1_000_000;

/// Maximum decoded string length in bytes.
pub const MAX_STRING_LEN: usize = 64 * 1024 * 1024;

/// Maximum decoded bytes payload length.
pub const MAX_BYTES_LEN: usize = 64 * 1024 * 1024;

/// Maximum index table bucket count.
pub const MAX_INDEX_BUCKETS: usize = 1_000_000;

/// Maximum index table entry count.
pub const MAX_INDEX_ENTRIES: usize = 1_000_000;

/// Maximum field-name length stored in an index bucket.
pub const MAX_FIELD_NAME_LEN: usize = 64 * 1024;

/// Maximum decompressed size for LZ4 sections (semantic or data).
pub const MAX_DECOMPRESSED_SIZE: i32 = 256 * 1024 * 1024;

/// Maximum nesting depth for composite values (list/map/struct/union/optional).
///
/// Guards against stack-overflow DoS from adversarially deep trees during
/// encode, decode, conversion, schema validation, and security walks.
pub const MAX_NESTING_DEPTH: usize = 256;

/// Ensure a decoded count does not exceed a hard limit.
pub fn ensure_count(count: usize, max: usize, what: &str) -> Result<()> {
    if count > max {
        return Err(BCSError::Format(format!(
            "{} count {} exceeds limit {}",
            what, count, max
        )));
    }
    Ok(())
}

/// Ensure a nesting depth does not exceed [`MAX_NESTING_DEPTH`].
pub fn ensure_depth(depth: usize) -> Result<()> {
    if depth > MAX_NESTING_DEPTH {
        return Err(BCSError::Format(format!(
            "Nesting depth {} exceeds limit {}",
            depth, MAX_NESTING_DEPTH
        )));
    }
    Ok(())
}

/// Convert a section offset/size into a checked byte range within `file_len`.
pub fn checked_section_range(file_len: usize, offset: u64, size: u64) -> Result<(usize, usize)> {
    let offset = usize::try_from(offset).map_err(|_| {
        BCSError::Format(format!(
            "Section offset {} does not fit in address space",
            offset
        ))
    })?;
    let size = usize::try_from(size).map_err(|_| {
        BCSError::Format(format!(
            "Section size {} does not fit in address space",
            size
        ))
    })?;
    let end = offset.checked_add(size).ok_or_else(|| {
        BCSError::Format(format!(
            "Section offset/size overflow: offset={}, size={}",
            offset, size
        ))
    })?;
    if end > file_len {
        return Err(BCSError::Format(format!(
            "Section out of bounds: end={}, file_len={}",
            end, file_len
        )));
    }
    Ok((offset, end))
}

/// Allocate a zeroed buffer only after validating the requested length.
pub fn alloc_buf(len: usize, max: usize, what: &str) -> Result<Vec<u8>> {
    ensure_count(len, max, what)?;
    Ok(vec![0u8; len])
}

/// Decompress an LZ4 block that was compressed with `prepend_size = true`,
/// rejecting declared uncompressed sizes above [`MAX_DECOMPRESSED_SIZE`].
pub fn decompress_lz4_limited(data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < 4 {
        return Err(BCSError::Decoding(
            "LZ4 payload too short to contain size prefix".to_string(),
        ));
    }

    let declared = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    if !(0..=MAX_DECOMPRESSED_SIZE).contains(&declared) {
        return Err(BCSError::Decoding(format!(
            "LZ4 declared size {} exceeds limit {}",
            declared, MAX_DECOMPRESSED_SIZE
        )));
    }

    lz4::block::decompress(data, None)
        .map_err(|e| BCSError::Decoding(format!("LZ4 decompression failed: {}", e)))
}

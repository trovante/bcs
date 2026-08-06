// Type system implementation for BCS format

use crate::error::{BCSError, Result};
use crate::limits::{self, MAX_BYTES_LEN, MAX_COLLECTION_LEN, MAX_STRING_LEN};
use std::io::{Read, Write};

// ============================================================================
// Format Constants
// ============================================================================

/// Magic number for BCS files: "BCSF" (0x42435346)
pub const MAGIC_NUMBER: u32 = 0x42435346;

/// Current BCS format version
pub const VERSION_MAJOR: u8 = 1;
pub const VERSION_MINOR: u8 = 0;

/// Header size in bytes (fixed at 64 bytes)
pub const HEADER_SIZE: usize = 64;

// ============================================================================
// Type Tags
// ============================================================================

/// Type tags for binary encoding (1 byte)
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeTag {
    // Null and Boolean
    Null = 0x00,
    BoolFalse = 0x01,
    BoolTrue = 0x02,

    // Signed Integers
    Int8 = 0x10,
    Int16 = 0x11,
    Int32 = 0x12,
    Int64 = 0x13,

    // Unsigned Integers
    UInt8 = 0x14,
    UInt16 = 0x15,
    UInt32 = 0x16,
    UInt64 = 0x17,

    // Floating Point
    Float32 = 0x20,
    Float64 = 0x21,

    // Strings and Bytes
    StringInline = 0x30,   // length < 256
    StringExternal = 0x31, // length >= 256
    BytesInline = 0x32,
    BytesExternal = 0x33,
    /// Index into the file string table (STRUCTURAL_DEDUP). Payload: `u32` id.
    StringInterned = 0x34,

    // Composite Types
    List = 0x40,
    Map = 0x41,
    Struct = 0x42,
    Union = 0x43,
    OptionalSome = 0x44,
    OptionalNone = 0x45,
}

impl TypeTag {
    /// Convert a u8 to a TypeTag
    pub fn from_u8(value: u8) -> Result<Self> {
        match value {
            0x00 => Ok(TypeTag::Null),
            0x01 => Ok(TypeTag::BoolFalse),
            0x02 => Ok(TypeTag::BoolTrue),
            0x10 => Ok(TypeTag::Int8),
            0x11 => Ok(TypeTag::Int16),
            0x12 => Ok(TypeTag::Int32),
            0x13 => Ok(TypeTag::Int64),
            0x14 => Ok(TypeTag::UInt8),
            0x15 => Ok(TypeTag::UInt16),
            0x16 => Ok(TypeTag::UInt32),
            0x17 => Ok(TypeTag::UInt64),
            0x20 => Ok(TypeTag::Float32),
            0x21 => Ok(TypeTag::Float64),
            0x30 => Ok(TypeTag::StringInline),
            0x31 => Ok(TypeTag::StringExternal),
            0x32 => Ok(TypeTag::BytesInline),
            0x33 => Ok(TypeTag::BytesExternal),
            0x34 => Ok(TypeTag::StringInterned),
            0x40 => Ok(TypeTag::List),
            0x41 => Ok(TypeTag::Map),
            0x42 => Ok(TypeTag::Struct),
            0x43 => Ok(TypeTag::Union),
            0x44 => Ok(TypeTag::OptionalSome),
            0x45 => Ok(TypeTag::OptionalNone),
            _ => Err(BCSError::Decoding(format!(
                "Invalid type tag: 0x{:02X}",
                value
            ))),
        }
    }

    /// Convert TypeTag to u8
    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

// ============================================================================
// File Header
// ============================================================================

/// Flags for BCS file header
#[derive(Debug, Clone, Copy, Default)]
pub struct HeaderFlags {
    /// Semantic layer is compressed with LZ4
    pub compressed: bool,
    /// Reserved header bit `0x0002` (historically named AI_METADATA; unused).
    ///
    /// Writers must leave this clear. Readers may observe it on older files and
    /// must ignore it for semantics.
    pub ai_metadata: bool,
    /// Data layer is compressed with LZ4
    pub data_compressed: bool,
    /// String table section present between index and data (`STRUCTURAL_DEDUP`)
    pub structural_dedup: bool,
}

impl HeaderFlags {
    /// Convert flags to u16 bitfield
    pub fn to_u16(self) -> u16 {
        let mut flags = 0u16;
        if self.compressed {
            flags |= 0x0001;
        }
        if self.ai_metadata {
            flags |= 0x0002;
        }
        if self.data_compressed {
            flags |= 0x0004;
        }
        if self.structural_dedup {
            flags |= 0x0008;
        }
        flags
    }

    /// Parse flags from u16 bitfield
    pub fn from_u16(value: u16) -> Self {
        Self {
            compressed: (value & 0x0001) != 0,
            ai_metadata: (value & 0x0002) != 0,
            data_compressed: (value & 0x0004) != 0,
            structural_dedup: (value & 0x0008) != 0,
        }
    }
}

/// BCS file header (64 bytes fixed size)
#[derive(Debug, Clone)]
pub struct Header {
    /// Magic number: 0x42435346 ("BCSF")
    pub magic: u32,

    /// Major version
    pub version_major: u8,

    /// Minor version
    pub version_minor: u8,

    /// File flags
    pub flags: HeaderFlags,

    /// Offset to semantic layer
    pub semantic_offset: u64,

    /// Size of semantic layer in bytes
    pub semantic_size: u64,

    /// Offset to index table
    pub index_offset: u64,

    /// Size of index table in bytes
    pub index_size: u64,

    /// Offset to binary data layer
    pub data_offset: u64,

    /// Size of binary data layer in bytes
    pub data_size: u64,

    /// CRC64 checksum of entire file (excluding this field)
    pub checksum: u64,
}

impl Header {
    /// Create a new header with default values
    pub fn new() -> Self {
        Self {
            magic: MAGIC_NUMBER,
            version_major: VERSION_MAJOR,
            version_minor: VERSION_MINOR,
            flags: HeaderFlags::default(),
            semantic_offset: 0,
            semantic_size: 0,
            index_offset: 0,
            index_size: 0,
            data_offset: 0,
            data_size: 0,
            checksum: 0,
        }
    }

    /// Write header to a writer
    pub fn write<W: Write>(&self, writer: &mut W) -> Result<()> {
        // Magic number (4 bytes)
        writer.write_all(&self.magic.to_le_bytes())?;

        // Version (2 bytes)
        writer.write_all(&[self.version_major, self.version_minor])?;

        // Flags (2 bytes)
        writer.write_all(&self.flags.to_u16().to_le_bytes())?;

        // Offsets and sizes (6 * 8 bytes = 48 bytes)
        writer.write_all(&self.semantic_offset.to_le_bytes())?;
        writer.write_all(&self.semantic_size.to_le_bytes())?;
        writer.write_all(&self.index_offset.to_le_bytes())?;
        writer.write_all(&self.index_size.to_le_bytes())?;
        writer.write_all(&self.data_offset.to_le_bytes())?;
        writer.write_all(&self.data_size.to_le_bytes())?;

        // Checksum (8 bytes)
        writer.write_all(&self.checksum.to_le_bytes())?;

        Ok(())
    }

    /// Read header from a reader
    pub fn read<R: Read>(reader: &mut R) -> Result<Self> {
        let mut buf = [0u8; HEADER_SIZE];
        reader.read_exact(&mut buf)?;

        // Parse magic number
        let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        if magic != MAGIC_NUMBER {
            return Err(BCSError::Format(format!(
                "Invalid magic number: expected 0x{:08X}, got 0x{:08X}",
                MAGIC_NUMBER, magic
            )));
        }

        // Parse version
        let version_major = buf[4];
        let version_minor = buf[5];

        // Parse flags
        let flags_raw = u16::from_le_bytes([buf[6], buf[7]]);
        let flags = HeaderFlags::from_u16(flags_raw);

        // Parse offsets and sizes
        let semantic_offset = u64::from_le_bytes(buf[8..16].try_into().unwrap());
        let semantic_size = u64::from_le_bytes(buf[16..24].try_into().unwrap());
        let index_offset = u64::from_le_bytes(buf[24..32].try_into().unwrap());
        let index_size = u64::from_le_bytes(buf[32..40].try_into().unwrap());
        let data_offset = u64::from_le_bytes(buf[40..48].try_into().unwrap());
        let data_size = u64::from_le_bytes(buf[48..56].try_into().unwrap());

        // Parse checksum
        let checksum = u64::from_le_bytes(buf[56..64].try_into().unwrap());

        Ok(Self {
            magic,
            version_major,
            version_minor,
            flags,
            semantic_offset,
            semantic_size,
            index_offset,
            index_size,
            data_offset,
            data_size,
            checksum,
        })
    }

    /// Validate header fields
    pub fn validate(&self) -> Result<()> {
        if self.magic != MAGIC_NUMBER {
            return Err(BCSError::Format("Invalid magic number".to_string()));
        }

        if self.version_major != VERSION_MAJOR {
            return Err(BCSError::Format(format!(
                "Unsupported version: {}.{}",
                self.version_major, self.version_minor
            )));
        }

        Ok(())
    }
}

impl Default for Header {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Endianness Utilities
// ============================================================================

/// Endianness for binary encoding
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endianness {
    Little,
    Big,
}

/// Default endianness for BCS format (little-endian)
pub const DEFAULT_ENDIANNESS: Endianness = Endianness::Little;

/// Utility functions for endianness handling
pub mod endian {
    use super::Endianness;

    /// Read i16 with specified endianness
    pub fn read_i16(bytes: &[u8], endian: Endianness) -> i16 {
        match endian {
            Endianness::Little => i16::from_le_bytes([bytes[0], bytes[1]]),
            Endianness::Big => i16::from_be_bytes([bytes[0], bytes[1]]),
        }
    }

    /// Read i32 with specified endianness
    pub fn read_i32(bytes: &[u8], endian: Endianness) -> i32 {
        match endian {
            Endianness::Little => i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            Endianness::Big => i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        }
    }

    /// Read i64 with specified endianness
    pub fn read_i64(bytes: &[u8], endian: Endianness) -> i64 {
        match endian {
            Endianness::Little => i64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]),
            Endianness::Big => i64::from_be_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]),
        }
    }

    /// Read u16 with specified endianness
    pub fn read_u16(bytes: &[u8], endian: Endianness) -> u16 {
        match endian {
            Endianness::Little => u16::from_le_bytes([bytes[0], bytes[1]]),
            Endianness::Big => u16::from_be_bytes([bytes[0], bytes[1]]),
        }
    }

    /// Read u32 with specified endianness
    pub fn read_u32(bytes: &[u8], endian: Endianness) -> u32 {
        match endian {
            Endianness::Little => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            Endianness::Big => u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        }
    }

    /// Read u64 with specified endianness
    pub fn read_u64(bytes: &[u8], endian: Endianness) -> u64 {
        match endian {
            Endianness::Little => u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]),
            Endianness::Big => u64::from_be_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]),
        }
    }

    /// Read f32 with specified endianness
    pub fn read_f32(bytes: &[u8], endian: Endianness) -> f32 {
        let bits = read_u32(bytes, endian);
        f32::from_bits(bits)
    }

    /// Read f64 with specified endianness
    pub fn read_f64(bytes: &[u8], endian: Endianness) -> f64 {
        let bits = read_u64(bytes, endian);
        f64::from_bits(bits)
    }

    /// Write i16 with specified endianness
    pub fn write_i16(value: i16, endian: Endianness) -> [u8; 2] {
        match endian {
            Endianness::Little => value.to_le_bytes(),
            Endianness::Big => value.to_be_bytes(),
        }
    }

    /// Write i32 with specified endianness
    pub fn write_i32(value: i32, endian: Endianness) -> [u8; 4] {
        match endian {
            Endianness::Little => value.to_le_bytes(),
            Endianness::Big => value.to_be_bytes(),
        }
    }

    /// Write i64 with specified endianness
    pub fn write_i64(value: i64, endian: Endianness) -> [u8; 8] {
        match endian {
            Endianness::Little => value.to_le_bytes(),
            Endianness::Big => value.to_be_bytes(),
        }
    }

    /// Write u16 with specified endianness
    pub fn write_u16(value: u16, endian: Endianness) -> [u8; 2] {
        match endian {
            Endianness::Little => value.to_le_bytes(),
            Endianness::Big => value.to_be_bytes(),
        }
    }

    /// Write u32 with specified endianness
    pub fn write_u32(value: u32, endian: Endianness) -> [u8; 4] {
        match endian {
            Endianness::Little => value.to_le_bytes(),
            Endianness::Big => value.to_be_bytes(),
        }
    }

    /// Write u64 with specified endianness
    pub fn write_u64(value: u64, endian: Endianness) -> [u8; 8] {
        match endian {
            Endianness::Little => value.to_le_bytes(),
            Endianness::Big => value.to_be_bytes(),
        }
    }

    /// Write f32 with specified endianness
    pub fn write_f32(value: f32, endian: Endianness) -> [u8; 4] {
        write_u32(value.to_bits(), endian)
    }

    /// Write f64 with specified endianness
    pub fn write_f64(value: f64, endian: Endianness) -> [u8; 8] {
        write_u64(value.to_bits(), endian)
    }
}

// ============================================================================
// Binary Primitives Encoding/Decoding
// ============================================================================

/// Encoder for binary primitives
pub struct PrimitiveEncoder {
    endianness: Endianness,
}

impl PrimitiveEncoder {
    /// Create a new encoder with default endianness
    pub fn new() -> Self {
        Self {
            endianness: DEFAULT_ENDIANNESS,
        }
    }

    /// Create a new encoder with specified endianness
    pub fn with_endianness(endianness: Endianness) -> Self {
        Self { endianness }
    }

    // Null encoding
    pub fn encode_null<W: Write>(&self, writer: &mut W) -> Result<()> {
        writer.write_all(&[TypeTag::Null.to_u8()])?;
        Ok(())
    }

    // Boolean encoding
    pub fn encode_bool<W: Write>(&self, writer: &mut W, value: bool) -> Result<()> {
        let tag = if value {
            TypeTag::BoolTrue
        } else {
            TypeTag::BoolFalse
        };
        writer.write_all(&[tag.to_u8()])?;
        Ok(())
    }

    // Signed integer encoding
    pub fn encode_i8<W: Write>(&self, writer: &mut W, value: i8) -> Result<()> {
        writer.write_all(&[TypeTag::Int8.to_u8()])?;
        writer.write_all(&[value as u8])?;
        Ok(())
    }

    pub fn encode_i16<W: Write>(&self, writer: &mut W, value: i16) -> Result<()> {
        writer.write_all(&[TypeTag::Int16.to_u8()])?;
        writer.write_all(&endian::write_i16(value, self.endianness))?;
        Ok(())
    }

    pub fn encode_i32<W: Write>(&self, writer: &mut W, value: i32) -> Result<()> {
        writer.write_all(&[TypeTag::Int32.to_u8()])?;
        writer.write_all(&endian::write_i32(value, self.endianness))?;
        Ok(())
    }

    pub fn encode_i64<W: Write>(&self, writer: &mut W, value: i64) -> Result<()> {
        writer.write_all(&[TypeTag::Int64.to_u8()])?;
        writer.write_all(&endian::write_i64(value, self.endianness))?;
        Ok(())
    }

    // Unsigned integer encoding
    pub fn encode_u8<W: Write>(&self, writer: &mut W, value: u8) -> Result<()> {
        writer.write_all(&[TypeTag::UInt8.to_u8()])?;
        writer.write_all(&[value])?;
        Ok(())
    }

    pub fn encode_u16<W: Write>(&self, writer: &mut W, value: u16) -> Result<()> {
        writer.write_all(&[TypeTag::UInt16.to_u8()])?;
        writer.write_all(&endian::write_u16(value, self.endianness))?;
        Ok(())
    }

    pub fn encode_u32<W: Write>(&self, writer: &mut W, value: u32) -> Result<()> {
        writer.write_all(&[TypeTag::UInt32.to_u8()])?;
        writer.write_all(&endian::write_u32(value, self.endianness))?;
        Ok(())
    }

    pub fn encode_u64<W: Write>(&self, writer: &mut W, value: u64) -> Result<()> {
        writer.write_all(&[TypeTag::UInt64.to_u8()])?;
        writer.write_all(&endian::write_u64(value, self.endianness))?;
        Ok(())
    }

    // Floating point encoding
    pub fn encode_f32<W: Write>(&self, writer: &mut W, value: f32) -> Result<()> {
        writer.write_all(&[TypeTag::Float32.to_u8()])?;
        writer.write_all(&endian::write_f32(value, self.endianness))?;
        Ok(())
    }

    pub fn encode_f64<W: Write>(&self, writer: &mut W, value: f64) -> Result<()> {
        writer.write_all(&[TypeTag::Float64.to_u8()])?;
        writer.write_all(&endian::write_f64(value, self.endianness))?;
        Ok(())
    }

    // String encoding (UTF-8, length-prefixed)
    pub fn encode_string<W: Write>(&self, writer: &mut W, value: &str) -> Result<()> {
        let bytes = value.as_bytes();
        let len = bytes.len();

        if len < 256 {
            // Inline string
            writer.write_all(&[TypeTag::StringInline.to_u8()])?;
            writer.write_all(&[len as u8])?;
        } else {
            // External string with uint32 length
            writer.write_all(&[TypeTag::StringExternal.to_u8()])?;
            writer.write_all(&(len as u32).to_le_bytes())?;
        }

        writer.write_all(bytes)?;
        Ok(())
    }

    // Bytes encoding (length-prefixed)
    pub fn encode_bytes<W: Write>(&self, writer: &mut W, value: &[u8]) -> Result<()> {
        let len = value.len();

        if len < 256 {
            // Inline bytes
            writer.write_all(&[TypeTag::BytesInline.to_u8()])?;
            writer.write_all(&[len as u8])?;
        } else {
            // External bytes with uint32 length
            writer.write_all(&[TypeTag::BytesExternal.to_u8()])?;
            writer.write_all(&(len as u32).to_le_bytes())?;
        }

        writer.write_all(value)?;
        Ok(())
    }

    // Varint encoding (LEB128 format)
    pub fn encode_varint<W: Write>(&self, writer: &mut W, mut value: u64) -> Result<()> {
        loop {
            let mut byte = (value & 0x7F) as u8;
            value >>= 7;

            if value != 0 {
                byte |= 0x80; // Set continuation bit
            }

            writer.write_all(&[byte])?;

            if value == 0 {
                break;
            }
        }
        Ok(())
    }
}

impl Default for PrimitiveEncoder {
    fn default() -> Self {
        Self::new()
    }
}

/// Decoder for binary primitives
pub struct PrimitiveDecoder {
    endianness: Endianness,
}

impl PrimitiveDecoder {
    /// Create a new decoder with default endianness
    pub fn new() -> Self {
        Self {
            endianness: DEFAULT_ENDIANNESS,
        }
    }

    /// Create a new decoder with specified endianness
    pub fn with_endianness(endianness: Endianness) -> Self {
        Self { endianness }
    }

    /// Read and verify type tag
    fn read_type_tag<R: Read>(&self, reader: &mut R, expected: TypeTag) -> Result<()> {
        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf)?;
        let tag = TypeTag::from_u8(buf[0])?;

        if tag != expected {
            return Err(BCSError::Decoding(format!(
                "Type mismatch: expected {:?}, got {:?}",
                expected, tag
            )));
        }

        Ok(())
    }

    // Null decoding
    pub fn decode_null<R: Read>(&self, reader: &mut R) -> Result<()> {
        self.read_type_tag(reader, TypeTag::Null)?;
        Ok(())
    }

    // Boolean decoding
    pub fn decode_bool<R: Read>(&self, reader: &mut R) -> Result<bool> {
        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf)?;
        let tag = TypeTag::from_u8(buf[0])?;

        match tag {
            TypeTag::BoolFalse => Ok(false),
            TypeTag::BoolTrue => Ok(true),
            _ => Err(BCSError::Decoding(format!(
                "Expected boolean type tag, got {:?}",
                tag
            ))),
        }
    }

    // Signed integer decoding
    pub fn decode_i8<R: Read>(&self, reader: &mut R) -> Result<i8> {
        self.read_type_tag(reader, TypeTag::Int8)?;
        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf)?;
        Ok(buf[0] as i8)
    }

    pub fn decode_i16<R: Read>(&self, reader: &mut R) -> Result<i16> {
        self.read_type_tag(reader, TypeTag::Int16)?;
        let mut buf = [0u8; 2];
        reader.read_exact(&mut buf)?;
        Ok(endian::read_i16(&buf, self.endianness))
    }

    pub fn decode_i32<R: Read>(&self, reader: &mut R) -> Result<i32> {
        self.read_type_tag(reader, TypeTag::Int32)?;
        let mut buf = [0u8; 4];
        reader.read_exact(&mut buf)?;
        Ok(endian::read_i32(&buf, self.endianness))
    }

    pub fn decode_i64<R: Read>(&self, reader: &mut R) -> Result<i64> {
        self.read_type_tag(reader, TypeTag::Int64)?;
        let mut buf = [0u8; 8];
        reader.read_exact(&mut buf)?;
        Ok(endian::read_i64(&buf, self.endianness))
    }

    // Unsigned integer decoding
    pub fn decode_u8<R: Read>(&self, reader: &mut R) -> Result<u8> {
        self.read_type_tag(reader, TypeTag::UInt8)?;
        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf)?;
        Ok(buf[0])
    }

    pub fn decode_u16<R: Read>(&self, reader: &mut R) -> Result<u16> {
        self.read_type_tag(reader, TypeTag::UInt16)?;
        let mut buf = [0u8; 2];
        reader.read_exact(&mut buf)?;
        Ok(endian::read_u16(&buf, self.endianness))
    }

    pub fn decode_u32<R: Read>(&self, reader: &mut R) -> Result<u32> {
        self.read_type_tag(reader, TypeTag::UInt32)?;
        let mut buf = [0u8; 4];
        reader.read_exact(&mut buf)?;
        Ok(endian::read_u32(&buf, self.endianness))
    }

    pub fn decode_u64<R: Read>(&self, reader: &mut R) -> Result<u64> {
        self.read_type_tag(reader, TypeTag::UInt64)?;
        let mut buf = [0u8; 8];
        reader.read_exact(&mut buf)?;
        Ok(endian::read_u64(&buf, self.endianness))
    }

    // Floating point decoding
    pub fn decode_f32<R: Read>(&self, reader: &mut R) -> Result<f32> {
        self.read_type_tag(reader, TypeTag::Float32)?;
        let mut buf = [0u8; 4];
        reader.read_exact(&mut buf)?;
        Ok(endian::read_f32(&buf, self.endianness))
    }

    pub fn decode_f64<R: Read>(&self, reader: &mut R) -> Result<f64> {
        self.read_type_tag(reader, TypeTag::Float64)?;
        let mut buf = [0u8; 8];
        reader.read_exact(&mut buf)?;
        Ok(endian::read_f64(&buf, self.endianness))
    }

    // String decoding (UTF-8, length-prefixed)
    pub fn decode_string<R: Read>(&self, reader: &mut R) -> Result<String> {
        let mut tag_buf = [0u8; 1];
        reader.read_exact(&mut tag_buf)?;
        let tag = TypeTag::from_u8(tag_buf[0])?;

        let len = match tag {
            TypeTag::StringInline => {
                let mut len_buf = [0u8; 1];
                reader.read_exact(&mut len_buf)?;
                len_buf[0] as usize
            }
            TypeTag::StringExternal => {
                let mut len_buf = [0u8; 4];
                reader.read_exact(&mut len_buf)?;
                u32::from_le_bytes(len_buf) as usize
            }
            _ => {
                return Err(BCSError::Decoding(format!(
                    "Expected string type tag, got {:?}",
                    tag
                )));
            }
        };

        let mut buf = limits::alloc_buf(len, MAX_STRING_LEN, "String")?;
        reader.read_exact(&mut buf)?;

        String::from_utf8(buf)
            .map_err(|e| BCSError::Decoding(format!("Invalid UTF-8 string: {}", e)))
    }

    // Bytes decoding (length-prefixed)
    pub fn decode_bytes<R: Read>(&self, reader: &mut R) -> Result<Vec<u8>> {
        let mut tag_buf = [0u8; 1];
        reader.read_exact(&mut tag_buf)?;
        let tag = TypeTag::from_u8(tag_buf[0])?;

        let len = match tag {
            TypeTag::BytesInline => {
                let mut len_buf = [0u8; 1];
                reader.read_exact(&mut len_buf)?;
                len_buf[0] as usize
            }
            TypeTag::BytesExternal => {
                let mut len_buf = [0u8; 4];
                reader.read_exact(&mut len_buf)?;
                u32::from_le_bytes(len_buf) as usize
            }
            _ => {
                return Err(BCSError::Decoding(format!(
                    "Expected bytes type tag, got {:?}",
                    tag
                )));
            }
        };

        let mut buf = limits::alloc_buf(len, MAX_BYTES_LEN, "Bytes")?;
        reader.read_exact(&mut buf)?;
        Ok(buf)
    }

    // Varint decoding (LEB128 format)
    pub fn decode_varint<R: Read>(&self, reader: &mut R) -> Result<u64> {
        let mut result = 0u64;
        let mut shift = 0;

        loop {
            let mut buf = [0u8; 1];
            reader.read_exact(&mut buf)?;
            let byte = buf[0];

            result |= ((byte & 0x7F) as u64) << shift;

            if (byte & 0x80) == 0 {
                break;
            }

            shift += 7;

            if shift >= 64 {
                return Err(BCSError::Decoding("Varint overflow".to_string()));
            }
        }

        Ok(result)
    }
}

impl Default for PrimitiveDecoder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Composite Types Encoding/Decoding
// ============================================================================

/// Value type for composite structures
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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
    Struct(Vec<(String, u64, Value)>), // (field_name, hash, value) tuples
    Union(u32, Box<Value>),            // (variant_tag, value)
    Optional(Option<Box<Value>>),
}

/// Encoder for composite types
pub struct CompositeEncoder {
    primitive: PrimitiveEncoder,
    string_table: Option<std::sync::Arc<crate::string_table::StringTable>>,
}

impl CompositeEncoder {
    /// Create a new composite encoder
    pub fn new() -> Self {
        Self {
            primitive: PrimitiveEncoder::new(),
            string_table: None,
        }
    }

    /// Create a new composite encoder with specified endianness
    pub fn with_endianness(endianness: Endianness) -> Self {
        Self {
            primitive: PrimitiveEncoder::with_endianness(endianness),
            string_table: None,
        }
    }

    /// Attach a string table for STRUCTURAL_DEDUP interning.
    pub fn with_string_table(
        mut self,
        table: std::sync::Arc<crate::string_table::StringTable>,
    ) -> Self {
        self.string_table = Some(table);
        self
    }

    fn encode_maybe_interned_string<W: Write>(&self, writer: &mut W, value: &str) -> Result<()> {
        if let Some(table) = &self.string_table {
            if let Some(id) = table.lookup_id(value) {
                writer.write_all(&[TypeTag::StringInterned.to_u8()])?;
                writer.write_all(&id.to_le_bytes())?;
                return Ok(());
            }
        }
        self.primitive.encode_string(writer, value)
    }

    /// Encode a value (dispatches to appropriate encoder)
    pub fn encode_value<W: Write>(&self, writer: &mut W, value: &Value) -> Result<()> {
        self.encode_value_at(writer, value, 0)
    }

    fn encode_value_at<W: Write>(&self, writer: &mut W, value: &Value, depth: usize) -> Result<()> {
        limits::ensure_depth(depth)?;
        match value {
            Value::Null => self.primitive.encode_null(writer),
            Value::Bool(v) => self.primitive.encode_bool(writer, *v),
            Value::Int8(v) => self.primitive.encode_i8(writer, *v),
            Value::Int16(v) => self.primitive.encode_i16(writer, *v),
            Value::Int32(v) => self.primitive.encode_i32(writer, *v),
            Value::Int64(v) => self.primitive.encode_i64(writer, *v),
            Value::UInt8(v) => self.primitive.encode_u8(writer, *v),
            Value::UInt16(v) => self.primitive.encode_u16(writer, *v),
            Value::UInt32(v) => self.primitive.encode_u32(writer, *v),
            Value::UInt64(v) => self.primitive.encode_u64(writer, *v),
            Value::Float32(v) => self.primitive.encode_f32(writer, *v),
            Value::Float64(v) => self.primitive.encode_f64(writer, *v),
            Value::String(v) => self.encode_maybe_interned_string(writer, v),
            Value::Bytes(v) => self.primitive.encode_bytes(writer, v),
            Value::List(v) => self.encode_list_at(writer, v, depth),
            Value::Map(v) => self.encode_map_at(writer, v, depth),
            Value::Struct(v) => self.encode_struct_at(writer, v, depth),
            Value::Union(tag, v) => self.encode_union_at(writer, *tag, v, depth),
            Value::Optional(v) => self.encode_optional_at(writer, v, depth),
        }
    }

    /// Encode a list
    pub fn encode_list<W: Write>(&self, writer: &mut W, values: &[Value]) -> Result<()> {
        self.encode_list_at(writer, values, 0)
    }

    fn encode_list_at<W: Write>(
        &self,
        writer: &mut W,
        values: &[Value],
        depth: usize,
    ) -> Result<()> {
        writer.write_all(&[TypeTag::List.to_u8()])?;
        writer.write_all(&(values.len() as u32).to_le_bytes())?;
        for value in values {
            self.encode_value_at(writer, value, depth + 1)?;
        }
        Ok(())
    }

    /// Encode a map
    pub fn encode_map<W: Write>(&self, writer: &mut W, entries: &[(Value, Value)]) -> Result<()> {
        self.encode_map_at(writer, entries, 0)
    }

    fn encode_map_at<W: Write>(
        &self,
        writer: &mut W,
        entries: &[(Value, Value)],
        depth: usize,
    ) -> Result<()> {
        writer.write_all(&[TypeTag::Map.to_u8()])?;
        writer.write_all(&(entries.len() as u32).to_le_bytes())?;
        for (key, value) in entries {
            self.encode_value_at(writer, key, depth + 1)?;
            self.encode_value_at(writer, value, depth + 1)?;
        }
        Ok(())
    }

    /// Encode a struct
    pub fn encode_struct<W: Write>(
        &self,
        writer: &mut W,
        fields: &[(String, u64, Value)],
    ) -> Result<()> {
        self.encode_struct_at(writer, fields, 0)
    }

    fn encode_struct_at<W: Write>(
        &self,
        writer: &mut W,
        fields: &[(String, u64, Value)],
        depth: usize,
    ) -> Result<()> {
        writer.write_all(&[TypeTag::Struct.to_u8()])?;
        writer.write_all(&(fields.len() as u32).to_le_bytes())?;
        for (field_name, hash, value) in fields {
            self.encode_maybe_interned_string(writer, field_name)?;
            writer.write_all(&hash.to_le_bytes())?;
            self.encode_value_at(writer, value, depth + 1)?;
        }
        Ok(())
    }

    /// Encode a union
    pub fn encode_union<W: Write>(
        &self,
        writer: &mut W,
        variant_tag: u32,
        value: &Value,
    ) -> Result<()> {
        self.encode_union_at(writer, variant_tag, value, 0)
    }

    fn encode_union_at<W: Write>(
        &self,
        writer: &mut W,
        variant_tag: u32,
        value: &Value,
        depth: usize,
    ) -> Result<()> {
        writer.write_all(&[TypeTag::Union.to_u8()])?;
        writer.write_all(&variant_tag.to_le_bytes())?;
        self.encode_value_at(writer, value, depth + 1)?;
        Ok(())
    }

    /// Encode an optional value
    pub fn encode_optional<W: Write>(
        &self,
        writer: &mut W,
        value: &Option<Box<Value>>,
    ) -> Result<()> {
        self.encode_optional_at(writer, value, 0)
    }

    fn encode_optional_at<W: Write>(
        &self,
        writer: &mut W,
        value: &Option<Box<Value>>,
        depth: usize,
    ) -> Result<()> {
        match value {
            None => {
                writer.write_all(&[TypeTag::OptionalNone.to_u8()])?;
            }
            Some(v) => {
                writer.write_all(&[TypeTag::OptionalSome.to_u8()])?;
                self.encode_value_at(writer, v, depth + 1)?;
            }
        }
        Ok(())
    }
}

impl Default for CompositeEncoder {
    fn default() -> Self {
        Self::new()
    }
}

/// Decoder for composite types
pub struct CompositeDecoder {
    primitive: PrimitiveDecoder,
    string_table: Option<std::sync::Arc<crate::string_table::StringTable>>,
}

impl CompositeDecoder {
    /// Create a new composite decoder
    pub fn new() -> Self {
        Self {
            primitive: PrimitiveDecoder::new(),
            string_table: None,
        }
    }

    /// Create a new composite decoder with specified endianness
    pub fn with_endianness(endianness: Endianness) -> Self {
        Self {
            primitive: PrimitiveDecoder::with_endianness(endianness),
            string_table: None,
        }
    }

    /// Attach string table for resolving `StringInterned` tags.
    pub fn with_string_table(
        mut self,
        table: std::sync::Arc<crate::string_table::StringTable>,
    ) -> Self {
        self.string_table = Some(table);
        self
    }

    fn read_len<R: Read>(&self, reader: &mut R) -> Result<usize> {
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)?;
        let len = u32::from_le_bytes(len_buf) as usize;
        limits::ensure_count(len, MAX_COLLECTION_LEN, "Collection")?;
        Ok(len)
    }

    fn read_string_len<R: Read>(&self, reader: &mut R) -> Result<usize> {
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)?;
        let len = u32::from_le_bytes(len_buf) as usize;
        limits::ensure_count(len, MAX_STRING_LEN, "String")?;
        Ok(len)
    }

    fn read_bytes_len<R: Read>(&self, reader: &mut R) -> Result<usize> {
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)?;
        let len = u32::from_le_bytes(len_buf) as usize;
        limits::ensure_count(len, MAX_BYTES_LEN, "Bytes")?;
        Ok(len)
    }

    /// Decode a value (reads type tag and dispatches)
    pub fn decode_value<R: Read>(&self, reader: &mut R) -> Result<Value> {
        self.decode_value_at(reader, 0)
    }

    fn decode_value_at<R: Read>(&self, reader: &mut R, depth: usize) -> Result<Value> {
        limits::ensure_depth(depth)?;
        let mut tag_buf = [0u8; 1];
        reader.read_exact(&mut tag_buf)?;
        let tag = TypeTag::from_u8(tag_buf[0])?;

        match tag {
            TypeTag::Null => Ok(Value::Null),
            TypeTag::BoolFalse => Ok(Value::Bool(false)),
            TypeTag::BoolTrue => Ok(Value::Bool(true)),
            TypeTag::Int8 => {
                let mut buf = [0u8; 1];
                reader.read_exact(&mut buf)?;
                Ok(Value::Int8(buf[0] as i8))
            }
            TypeTag::Int16 => {
                let mut buf = [0u8; 2];
                reader.read_exact(&mut buf)?;
                Ok(Value::Int16(endian::read_i16(
                    &buf,
                    self.primitive.endianness,
                )))
            }
            TypeTag::Int32 => {
                let mut buf = [0u8; 4];
                reader.read_exact(&mut buf)?;
                Ok(Value::Int32(endian::read_i32(
                    &buf,
                    self.primitive.endianness,
                )))
            }
            TypeTag::Int64 => {
                let mut buf = [0u8; 8];
                reader.read_exact(&mut buf)?;
                Ok(Value::Int64(endian::read_i64(
                    &buf,
                    self.primitive.endianness,
                )))
            }
            TypeTag::UInt8 => {
                let mut buf = [0u8; 1];
                reader.read_exact(&mut buf)?;
                Ok(Value::UInt8(buf[0]))
            }
            TypeTag::UInt16 => {
                let mut buf = [0u8; 2];
                reader.read_exact(&mut buf)?;
                Ok(Value::UInt16(endian::read_u16(
                    &buf,
                    self.primitive.endianness,
                )))
            }
            TypeTag::UInt32 => {
                let mut buf = [0u8; 4];
                reader.read_exact(&mut buf)?;
                Ok(Value::UInt32(endian::read_u32(
                    &buf,
                    self.primitive.endianness,
                )))
            }
            TypeTag::UInt64 => {
                let mut buf = [0u8; 8];
                reader.read_exact(&mut buf)?;
                Ok(Value::UInt64(endian::read_u64(
                    &buf,
                    self.primitive.endianness,
                )))
            }
            TypeTag::Float32 => {
                let mut buf = [0u8; 4];
                reader.read_exact(&mut buf)?;
                Ok(Value::Float32(endian::read_f32(
                    &buf,
                    self.primitive.endianness,
                )))
            }
            TypeTag::Float64 => {
                let mut buf = [0u8; 8];
                reader.read_exact(&mut buf)?;
                Ok(Value::Float64(endian::read_f64(
                    &buf,
                    self.primitive.endianness,
                )))
            }
            TypeTag::StringInline => {
                let mut len_buf = [0u8; 1];
                reader.read_exact(&mut len_buf)?;
                let len = len_buf[0] as usize;

                let mut buf = limits::alloc_buf(len, MAX_STRING_LEN, "String")?;
                reader.read_exact(&mut buf)?;

                let s = String::from_utf8(buf)
                    .map_err(|e| BCSError::Decoding(format!("Invalid UTF-8 string: {}", e)))?;
                Ok(Value::String(s))
            }
            TypeTag::StringExternal => {
                let len = self.read_string_len(reader)?;

                let mut buf = limits::alloc_buf(len, MAX_STRING_LEN, "String")?;
                reader.read_exact(&mut buf)?;

                let s = String::from_utf8(buf)
                    .map_err(|e| BCSError::Decoding(format!("Invalid UTF-8 string: {}", e)))?;
                Ok(Value::String(s))
            }
            TypeTag::StringInterned => {
                let mut id_buf = [0u8; 4];
                reader.read_exact(&mut id_buf)?;
                let id = u32::from_le_bytes(id_buf);
                let table = self.string_table.as_ref().ok_or_else(|| {
                    BCSError::Decoding(
                        "Encountered interned string but file has no string table".into(),
                    )
                })?;
                Ok(Value::String(table.get(id)?.to_string()))
            }
            TypeTag::BytesInline => {
                let mut len_buf = [0u8; 1];
                reader.read_exact(&mut len_buf)?;
                let len = len_buf[0] as usize;

                let mut buf = limits::alloc_buf(len, MAX_BYTES_LEN, "Bytes")?;
                reader.read_exact(&mut buf)?;
                Ok(Value::Bytes(buf))
            }
            TypeTag::BytesExternal => {
                let len = self.read_bytes_len(reader)?;

                let mut buf = limits::alloc_buf(len, MAX_BYTES_LEN, "Bytes")?;
                reader.read_exact(&mut buf)?;
                Ok(Value::Bytes(buf))
            }
            TypeTag::List => self.decode_list_at(reader, depth),
            TypeTag::Map => self.decode_map_at(reader, depth),
            TypeTag::Struct => self.decode_struct_at(reader, depth),
            TypeTag::Union => self.decode_union_at(reader, depth),
            TypeTag::OptionalSome => {
                let value = self.decode_value_at(reader, depth + 1)?;
                Ok(Value::Optional(Some(Box::new(value))))
            }
            TypeTag::OptionalNone => Ok(Value::Optional(None)),
        }
    }

    /// Decode a list
    pub fn decode_list<R: Read>(&self, reader: &mut R) -> Result<Value> {
        self.decode_list_at(reader, 0)
    }

    fn decode_list_at<R: Read>(&self, reader: &mut R, depth: usize) -> Result<Value> {
        let len = self.read_len(reader)?;
        let mut values = Vec::with_capacity(len);
        for _ in 0..len {
            values.push(self.decode_value_at(reader, depth + 1)?);
        }
        Ok(Value::List(values))
    }

    /// Decode a map
    pub fn decode_map<R: Read>(&self, reader: &mut R) -> Result<Value> {
        self.decode_map_at(reader, 0)
    }

    fn decode_map_at<R: Read>(&self, reader: &mut R, depth: usize) -> Result<Value> {
        let len = self.read_len(reader)?;
        let mut entries = Vec::with_capacity(len);
        for _ in 0..len {
            let key = self.decode_value_at(reader, depth + 1)?;
            let value = self.decode_value_at(reader, depth + 1)?;
            entries.push((key, value));
        }
        Ok(Value::Map(entries))
    }

    /// Decode a struct
    pub fn decode_struct<R: Read>(&self, reader: &mut R) -> Result<Value> {
        self.decode_struct_at(reader, 0)
    }

    fn decode_maybe_interned_string<R: Read>(&self, reader: &mut R) -> Result<String> {
        let mut tag_buf = [0u8; 1];
        reader.read_exact(&mut tag_buf)?;
        let tag = TypeTag::from_u8(tag_buf[0])?;
        match tag {
            TypeTag::StringInline => {
                let mut len_buf = [0u8; 1];
                reader.read_exact(&mut len_buf)?;
                let len = len_buf[0] as usize;
                let mut buf = limits::alloc_buf(len, MAX_STRING_LEN, "String")?;
                reader.read_exact(&mut buf)?;
                String::from_utf8(buf)
                    .map_err(|e| BCSError::Decoding(format!("Invalid UTF-8 string: {}", e)))
            }
            TypeTag::StringExternal => {
                let mut len_buf = [0u8; 4];
                reader.read_exact(&mut len_buf)?;
                let len = u32::from_le_bytes(len_buf) as usize;
                let mut buf = limits::alloc_buf(len, MAX_STRING_LEN, "String")?;
                reader.read_exact(&mut buf)?;
                String::from_utf8(buf)
                    .map_err(|e| BCSError::Decoding(format!("Invalid UTF-8 string: {}", e)))
            }
            TypeTag::StringInterned => {
                let mut id_buf = [0u8; 4];
                reader.read_exact(&mut id_buf)?;
                let id = u32::from_le_bytes(id_buf);
                let table = self.string_table.as_ref().ok_or_else(|| {
                    BCSError::Decoding(
                        "Encountered interned string but file has no string table".into(),
                    )
                })?;
                Ok(table.get(id)?.to_string())
            }
            _ => Err(BCSError::Decoding(format!(
                "Expected string type tag, got {:?}",
                tag
            ))),
        }
    }

    fn decode_struct_at<R: Read>(&self, reader: &mut R, depth: usize) -> Result<Value> {
        let count = self.read_len(reader)?;
        let mut fields = Vec::with_capacity(count);
        for _ in 0..count {
            let field_name = self.decode_maybe_interned_string(reader)?;
            let mut hash_buf = [0u8; 8];
            reader.read_exact(&mut hash_buf)?;
            let hash = u64::from_le_bytes(hash_buf);
            let value = self.decode_value_at(reader, depth + 1)?;
            fields.push((field_name, hash, value));
        }
        Ok(Value::Struct(fields))
    }

    /// Decode a union
    pub fn decode_union<R: Read>(&self, reader: &mut R) -> Result<Value> {
        self.decode_union_at(reader, 0)
    }

    fn decode_union_at<R: Read>(&self, reader: &mut R, depth: usize) -> Result<Value> {
        let mut tag_buf = [0u8; 4];
        reader.read_exact(&mut tag_buf)?;
        let variant_tag = u32::from_le_bytes(tag_buf);
        let value = self.decode_value_at(reader, depth + 1)?;
        Ok(Value::Union(variant_tag, Box::new(value)))
    }
}

impl Default for CompositeDecoder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_header_write_read() {
        let mut header = Header::new();
        header.semantic_offset = 64;
        header.semantic_size = 1024;
        header.index_offset = 1088;
        header.index_size = 512;
        header.data_offset = 1600;
        header.data_size = 2048;
        header.checksum = 0x123456789ABCDEF0;

        let mut buf = Vec::new();
        header.write(&mut buf).unwrap();

        assert_eq!(buf.len(), HEADER_SIZE);

        let mut cursor = Cursor::new(buf);
        let decoded = Header::read(&mut cursor).unwrap();

        assert_eq!(decoded.magic, MAGIC_NUMBER);
        assert_eq!(decoded.version_major, VERSION_MAJOR);
        assert_eq!(decoded.version_minor, VERSION_MINOR);
        assert_eq!(decoded.semantic_offset, 64);
        assert_eq!(decoded.semantic_size, 1024);
        assert_eq!(decoded.checksum, 0x123456789ABCDEF0);
    }

    #[test]
    fn test_primitive_encoding_decoding() {
        let encoder = PrimitiveEncoder::new();
        let decoder = PrimitiveDecoder::new();

        // Test integers
        let mut buf = Vec::new();
        encoder.encode_i32(&mut buf, 42).unwrap();
        let mut cursor = Cursor::new(buf);
        assert_eq!(decoder.decode_i32(&mut cursor).unwrap(), 42);

        // Test strings
        let mut buf = Vec::new();
        encoder.encode_string(&mut buf, "hello").unwrap();
        let mut cursor = Cursor::new(buf);
        assert_eq!(decoder.decode_string(&mut cursor).unwrap(), "hello");

        // Test booleans
        let mut buf = Vec::new();
        encoder.encode_bool(&mut buf, true).unwrap();
        let mut cursor = Cursor::new(buf);
        assert!(decoder.decode_bool(&mut cursor).unwrap());
    }

    #[test]
    fn test_varint_encoding_decoding() {
        let encoder = PrimitiveEncoder::new();
        let decoder = PrimitiveDecoder::new();

        let test_values = vec![0, 127, 128, 255, 256, 16383, 16384, u64::MAX];

        for value in test_values {
            let mut buf = Vec::new();
            encoder.encode_varint(&mut buf, value).unwrap();
            let mut cursor = Cursor::new(buf);
            assert_eq!(decoder.decode_varint(&mut cursor).unwrap(), value);
        }
    }

    #[test]
    fn test_composite_list_encoding_decoding() {
        let encoder = CompositeEncoder::new();
        let decoder = CompositeDecoder::new();

        let list = vec![Value::Int32(1), Value::Int32(2), Value::Int32(3)];

        let mut buf = Vec::new();
        encoder.encode_list(&mut buf, &list).unwrap();

        let mut cursor = Cursor::new(buf);
        let decoded = decoder.decode_value(&mut cursor).unwrap();

        assert_eq!(decoded, Value::List(list));
    }

    #[test]
    fn test_composite_map_encoding_decoding() {
        let encoder = CompositeEncoder::new();
        let decoder = CompositeDecoder::new();

        let map = vec![
            (Value::String("key1".to_string()), Value::Int32(100)),
            (Value::String("key2".to_string()), Value::Int32(200)),
        ];

        let mut buf = Vec::new();
        encoder.encode_map(&mut buf, &map).unwrap();

        let mut cursor = Cursor::new(buf);
        let decoded = decoder.decode_value(&mut cursor).unwrap();

        assert_eq!(decoded, Value::Map(map));
    }

    #[test]
    fn test_composite_optional_encoding_decoding() {
        let encoder = CompositeEncoder::new();
        let decoder = CompositeDecoder::new();

        // Test Some
        let some_value = Some(Box::new(Value::Int32(42)));
        let mut buf = Vec::new();
        encoder.encode_optional(&mut buf, &some_value).unwrap();
        let mut cursor = Cursor::new(buf);
        let decoded = decoder.decode_value(&mut cursor).unwrap();
        assert_eq!(decoded, Value::Optional(some_value));

        // Test None
        let none_value: Option<Box<Value>> = None;
        let mut buf = Vec::new();
        encoder.encode_optional(&mut buf, &none_value).unwrap();
        let mut cursor = Cursor::new(buf);
        let decoded = decoder.decode_value(&mut cursor).unwrap();
        assert_eq!(decoded, Value::Optional(None));
    }
}

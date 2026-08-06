# BCS File Format Specification v1.0

This document describes the binary layout currently implemented by this repository.

## 1. Overview

A BCS file is composed of contiguous sections:

1. Header (fixed 64 bytes)
2. Semantic layer (optional, may be compressed)
3. Index table (optional, sparse on disk)
4. String table (optional; present when `STRUCTURAL_DEDUP` is set)
5. Data layer (required, may be compressed)

All multi-byte numbers are little-endian.

## 2. Header

Header size is fixed at 64 bytes.

| Offset | Size | Field | Description |
|---|---:|---|---|
| 0x00 | 4 | magic | `0x42435346` (`"BCSF"`) |
| 0x04 | 1 | version_major | currently `1` |
| 0x05 | 1 | version_minor | currently `0` |
| 0x06 | 2 | flags | bitfield |
| 0x08 | 8 | semantic_offset | absolute file offset |
| 0x10 | 8 | semantic_size | bytes |
| 0x18 | 8 | index_offset | absolute file offset |
| 0x20 | 8 | index_size | bytes |
| 0x28 | 8 | data_offset | absolute file offset |
| 0x30 | 8 | data_size | bytes |
| 0x38 | 8 | checksum | CRC64 over file excluding checksum field |

Checksum exclusion window is bytes `56..64`.

### Checksum Algorithm

CRC64 with polynomial **ECMA-182** (`0xC96C5795D7870F42`):

1. Initialize `crc = 0xFFFFFFFFFFFFFFFF`
2. For each byte: `crc = (crc >> 8) ^ table[(crc ^ byte) & 0xFF]`
3. Final value: `!crc`

Coverage is the whole file except the checksum field itself (header bytes `0..56` followed by bytes `64..EOF`).

### Flags

| Bit | Mask | Name | Meaning |
|---:|---:|---|---|
| 0 | `0x0001` | COMPRESSION | semantic layer is LZ4 compressed |
| 1 | `0x0002` | RESERVED | reserved; writers **MUST** set to `0`; readers **MUST** ignore |
| 2 | `0x0004` | DATA_COMPRESSION | data layer is LZ4 compressed |
| 3 | `0x0008` | STRUCTURAL_DEDUP | string table section present; data may use `0x34` interned strings |

Bit `0x0002` was previously named `AI_METADATA` in early drafts but never carried embeddings or tags. It remains reserved for a future, explicitly versioned extension.

Readers that do not understand `STRUCTURAL_DEDUP` **MUST** fail closed when the flag is set (or when encountering type tag `0x34`).

## 3. Semantic Layer

The semantic layer stores schema bytes produced by `Schema::to_msgpack()`.

- If `COMPRESSION` is set, bytes are LZ4 block-compressed with embedded size (`lz4::block::compress(..., prepend_size = true)`).
- If semantic embedding is disabled, this section is empty (`semantic_size = 0`).

## 4. Index Table

Index table header:

| Field | Size |
|---|---:|
| entry_count | 4 |
| bucket_count | 4 |
| load_factor | 4 (f32) |

On-disk representation is sparse: only occupied buckets are written.

For each occupied bucket:

1. bucket index (`u32`)
2. bucket payload:
   - key hash (`u64`)
   - value offset (`u64`) relative to data-layer start
   - collision next pointer (`i32`)
   - field-name length (`u32`; `0` means no name stored)
   - field-name bytes (UTF-8, present only when length > 0)

### Key Hash

Field-name hashes use **XXHash64** with seed `0` over the UTF-8 field-name bytes.

If index embedding is disabled, this section is empty (`index_size = 0`).

## 5. String Table (optional)

Present only when `STRUCTURAL_DEDUP` (`0x0008`) is set. Lives between the index table and the data layer (`index_offset + index_size` through `data_offset`).

Wire layout:

| Field | Size | Description |
|---|---:|---|
| count | 4 | number of UTF-8 strings (`u32`) |
| entries | variable | `count` × (`len:u32` + UTF-8 bytes) |

Strings are stored in sorted order. Interned references in the data layer use zero-based `u32` indexes into this table.

When the flag is set but no strings qualify for interning, writers omit the flag and table (dedup is a no-op).

## 6. Data Layer

The data layer stores values using type tags and per-type payloads.

### Root Layout Modes

How the root value is stored depends on whether an index table is embedded:

| Mode | Condition | On-disk data-layer shape |
|---|---|---|
| Indexed | `index_size > 0` and root is a struct | Top-level fields are written as **separate values** (no wrapping `0x42` struct). Each index entry points at the start of that field's value. Full decode rebuilds a struct from index entries ordered by offset. |
| Single-value | no index, or non-struct root | One tagged value (may itself be a struct/list/map). |

Nested structs inside field values always use the normal struct encoding (`0x42`).

### Type Tags

| Tag | Type | Payload |
|---:|---|---|
| `0x00` | null | — |
| `0x01` | bool false | — |
| `0x02` | bool true | — |
| `0x10` | int8 | `i8` |
| `0x11` | int16 | `i16` |
| `0x12` | int32 | `i32` |
| `0x13` | int64 | `i64` |
| `0x14` | uint8 | `u8` |
| `0x15` | uint16 | `u16` |
| `0x16` | uint32 | `u32` |
| `0x17` | uint64 | `u64` |
| `0x20` | float32 | `f32` |
| `0x21` | float64 | `f64` |
| `0x30` | string inline | `len:u8` + UTF-8 bytes (`len < 256`) |
| `0x31` | string external | `len:u32` + UTF-8 bytes (`len >= 256`) |
| `0x32` | bytes inline | `len:u8` + bytes (`len < 256`) |
| `0x33` | bytes external | `len:u32` + bytes (`len >= 256`) |
| `0x34` | string interned | `id:u32` index into the string table (`STRUCTURAL_DEDUP` required) |
| `0x40` | list | `count:u32` + values |
| `0x41` | map | `count:u32` + key/value pairs |
| `0x42` | struct | `field_count:u32` + fields |
| `0x43` | union | `tag:u32` + value |
| `0x44` | optional some | value |
| `0x45` | optional none | — |

### Composite Encoding

- List: `[0x40][count:u32][value...]*count`
- Map: `[0x41][count:u32][key value]*count`
- Struct: `[0x42][count:u32][field_name:string][field_hash:u64][value]...`
  - `field_name` uses string tags (`0x30` / `0x31` / `0x34` when interned)
  - `field_hash` is XXHash64(seed 0) of the field name

### Nested index paths

When encoding with `--index-maps-over N`, writers may also register index entries for nested struct fields under parents with at least `N` fields (paths like `parent.child`). Map entry local indexes currently register the path at the map root offset (no per-entry child offsets without a further format change).

### Data Compression

When data compression is requested:

1. The encoder LZ4-compresses the data layer with embedded size (`prepend_size = true`).
2. Compressed bytes are kept **only when smaller** than the uncompressed layer.
3. `DATA_COMPRESSION` is set only in that case.

If `DATA_COMPRESSION` is set, the on-disk data section is the LZ4 block; otherwise it is raw tagged values.

## 7. Decoder Behavior

Decoder validation order (implemented behavior):

1. Read and validate header magic/version
2. Validate checksum
3. Decode semantic layer on demand
4. Decode index table on demand
5. Load string table when `STRUCTURAL_DEDUP` is set
6. Decode data layer (decompress first when `DATA_COMPRESSION` is set)

Notes:

- Path lookup and streaming decode decompress the data layer once (cached on the decoder) when `DATA_COMPRESSION` is set; offsets are relative to the uncompressed logical layer.
- Full decode also uses the same cached logical layer.
- With a non-empty index, full decode reconstructs the root struct from indexed field offsets (fail-fast on corrupt offsets or decode errors).
- Interned strings (`0x34`) resolve through the string table; path get treats them as ordinary strings.
- Composite values (list / map / struct / union / optional) must not nest deeper than **256** levels; deeper trees are rejected with a format error (encode and decode).

## 8. Compatibility Notes

- This specification reflects the current implementation in this repository.
- It intentionally avoids forward-looking guarantees and planned features.
- `STRUCTURAL_DEDUP` / `0x34` is opt-in at encode time; files without the flag remain unchanged.

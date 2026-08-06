# ADR: Opt-in structural dedup

**Status:** Implemented  
**Date:** 2026-08-05  
**Context:** P1-B in [roadmap-varlock-rx-ideas.md](../roadmap-varlock-rx-ideas.md)

## Decision

Introduce an **opt-in** string/key dictionary for repetitive configs:

- Header flag **`0x0008` (`STRUCTURAL_DEDUP`)** — file contains a string table section.
- Layout: after index, before data: `u32 count` + (`u32 len` + UTF-8)*count.
- New type tag **`0x34` (`StringInterned`)** — `u32` index into the table (used for string leaves and struct field names).
- Encode flags: `--dedup keys|strings|all` with `--dedup-min-repeats` / `--dedup-min-length`.
- Default **off** — existing files and benchmarks unchanged.
- Readers that do not understand `0x0008` must fail closed (unknown critical flag) or refuse decode when encountering `0x34`.

### Local nested indexes

Same milestone ships `--index-maps-over N`: for structs with ≥ N fields, register `parent.child` offsets in the top-level index. Map entries register the path at the map root (no stable per-entry child offsets without a further format change).

## Consequences

- Compatibility note in CHANGELOG and [spec/format.md](../../spec/format.md).
- Path get resolves interned strings transparently (string table + decompress cache).
- Dedup is a no-op when no string meets thresholds (flag not set).

## Non-goals

- Becoming a general RX-style write-once data store.
- Deduping across files or processes.
- Per-entry map child offsets in the index without a dedicated map-local index format.

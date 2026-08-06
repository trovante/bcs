# ADR: Compress + path access

**Status:** Accepted  
**Date:** 2026-08-05  
**Context:** P0-B in [roadmap-varlock-rx-ideas.md](../roadmap-varlock-rx-ideas.md)

## Decision

When the data layer is LZ4-compressed (`DATA_COMPRESSION`), path queries decompress the data layer **once** into a cached buffer on the `Decoder`, then perform indexed top-level lookup + nested walk against that logical layer (roadmap option **A**).

## Consequences

- Path get and streaming work with `--compress-data`.
- First path access pays decompression cost; subsequent gets on the same decoder reuse the cache.
- Offsets in the index remain relative to the **uncompressed** data layer (unchanged encode layout).
- No wire-format change; no encode-time rejection of compress+index.

## Alternatives rejected

- **B** Frame/chunk compression so on-disk offsets stay valid without full decompress — format change.
- **C** Disallow `--compress-data` with index — worse UX for size-sensitive configs that still need path reads.

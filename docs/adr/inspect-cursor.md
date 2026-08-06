# ADR: Offset-based Inspect cursor

**Status:** Implemented (F4b/F4c)  
**Date:** 2026-08-05  
**Context:** F4 in [roadmap-followups-inspect-gaps.md](../roadmap-followups-inspect-gaps.md)

## Decision

`InspectNode::from_decoder` uses an **offset-backed** path:

- Indexed files: root children = index table `(name, offset)` — no `decode_to_value`.
- Nested struct/list/map: enumerate children with wire layout + `Decoder::skip_value`.
- Leaves: decode only that value for preview; protect/secret → `[PROTECTED]` / `[SECRET_REF]`.
- `from_value` retained for tests and leaf/unknown-tag fallback.
- **No on-disk format change.**

## Consequences

- `inspect --tree` / `dump --format debug-tree` can avoid materializing a full `Value` tree (F4c acceptance).
- Path get (`Decoder::get`) remains the primary partial-read API; inspect is for ops DX.
- Bindings do not need RX-style Proxies.

## Alternatives rejected

- **Value-only inspect forever** — fails the large-file memory goal.
- **Frame/chunk compression for inspect** — format change; out of scope (see [compress-path.md](compress-path.md)).
- **Fail closed on any unknown tag** — prefer Value fallback per subtree until coverage is complete.

## Open questions (resolve in F4b)

1. Fallback policy: document which tags still use `from_value`.
2. Whether `scan` finding messages should redact matched substrings (security review F3 may decide independently).

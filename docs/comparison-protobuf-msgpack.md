# BCS vs Protobuf vs MessagePack

BCS is closer to MessagePack than to Protobuf, but it is a **config-oriented file container**, not a general-purpose RPC or messaging codec.

## One-line summary

| | Protobuf | MessagePack | BCS |
|---|---|---|---|
| What it is | Typed contract + wire format | Dynamic binary serialization (JSON-like) | Binary config container + optional layers |
| Mental model | Schema-first | Schema-less | Document-first (JSON / YAML / TOML → `.bcs`) |

## Comparison

| Dimension | Protobuf | MessagePack | BCS |
|---|---|---|---|
| **Schema** | Required (`.proto`); generates code | None; types live in the stream | Optional semantic layer (MessagePack); `sensitive_paths`; agent-safe export without values |
| **Types** | Static, versioned by field number | Dynamic (map / array / str / bin / …) | Document-style dynamic types; tags in the data layer |
| **Typical use** | gRPC, APIs, events, typed state | Cache, queues, compact interchange | Packaged configuration files |
| **Partial access** | Not native (full decode or custom) | Full decode | Index + path query (`--path` / `Decoder::get`); works with LZ4 data via decompress cache; optional nested map indexes |
| **Inspect** | External tooling | Libraries / pretty-printers | `inspect` / `show` / `dump` with offset cursor (no full-document materialize for tree view) |
| **Compression** | External (gzip, etc.) | External | Optional LZ4 on semantic / data layers |
| **Dedup** | N/A (field numbers, no names) | Application-level | Opt-in structural string/key table (`--dedup`) |
| **Integrity** | Not in the format | Not in the format | CRC64 in the header |
| **Field secrets / encryption** | No | No | Yes (`protect`, KMS, secret refs); `scan` / `run` for ops hygiene |
| **Ops CLI** | `protoc` + codegen tooling | Libraries + ad-hoc utilities | `encode` / `decode` / `validate` / `inspect` / `protect` / `reindex` / `scan` / `run` / `show` / `schema` |
| **Agents / MCP** | Ecosystem-dependent | N/A | Agent-safe schema + `bcs-mcp` tools (masked path get, scan, validate) |
| **Ecosystem** | Huge, mature, multi-language native | Very broad | Rust-first + in-tree FFI / bindings (Python, TS, Swift, C#, Java) |
| **Size** | Very compact (no field names on the wire) | Compact (maps still carry tags / names) | Variable; metadata / index can make files **larger** than JSON unless `--dedup` / compression help |
| **Schema evolution** | Field numbers, `reserved`, compatibility rules | Free-form (like JSON) | Project compatibility policy; not the Protobuf model |

## Where they overlap

**BCS ≈ MessagePack** at the data core: nested documents, dynamic typing, no mandatory codegen. BCS’s embedded schema layer itself uses MessagePack.

**BCS ≠ MessagePack** because BCS adds a config-file envelope: fixed header, CRC, optional index and string table, compression, field protect / secret refs, agent-safe schema, and an ops-oriented CLI (including scan/run).

**BCS ≠ Protobuf** because Protobuf is an **interface contract**. You change a `.proto`, regenerate stubs, and speak field `1`, `2`, `3`. BCS speaks `server.host` and starts from human-authored JSON / YAML / TOML.

## When to choose each

- **Protobuf** — APIs, microservices, stable cross-team contracts, maximum wire-size and compatibility control.
- **MessagePack** — “Binary JSON” for general payloads: fewer bytes, same mental model, no domain opinions.
- **BCS** — Packaging config for runtime / ops: validate, inspect, path query, encrypt fields, resolve external secrets, agent-safe contracts, CI scan.

## Verdict

Calling BCS “like Protobuf” is misleading: there is no codegen and no field-number model.

Calling it “MessagePack for configs” is closer but incomplete: BCS is a **layered file format** (header + schema + index + optional dedup table + data + security + agent/ops tooling), not only a codec.

Useful analogy: MessagePack is the *content shape*; BCS is the *file container* (plus CRC, index, protect, and agent-safe ops) designed for configuration—not a substitute for Protobuf in RPC.

# BCS vs Varlock vs RX

BCS overlaps with both projects in different ways: with **Varlock** on secrets, agent-safe schemas, and ops CLI hygiene, and with **RX** on indexed / path-oriented document access. They still solve different primary problems — but BCS has closed several gaps that earlier comparisons called out.

| Project | Link |
|---------|------|
| **BCS** | This repository — Binary Config Schema |
| **Varlock** | [dmno-dev/varlock](https://github.com/dmno-dev/varlock) |
| **RX** | [creationix/rx](https://github.com/creationix/rx) |

Related: [Roadmap: Varlock & RX ideas](roadmap-varlock-rx-ideas.md) (implemented) · [Follow-ups](roadmap-followups-inspect-gaps.md) (implemented)

## One-line summary

| | Varlock | RX | BCS |
|---|---|---|---|
| What it is | AI-safe `.env` schema + secret loading | Embedded random-access store for JSON-shaped data | Binary config container + security / index / agent layers |
| Mental model | Schema-first env vars for agents & apps | Encode once, query in place (no full parse) | Document-first (JSON / YAML / TOML → `.bcs`) |
| Primary artifact | `.env.schema` + env files | `.rx` / `.rxb` encoded documents | `.bcs` packaged configuration files |

## Comparison

| Dimension | Varlock | RX | BCS |
|---|---|---|---|
| **Problem space** | Env-var config, secret hygiene, AI-safe schemas | Large JSON-like data with sparse random reads | Packaged operational configuration |
| **Input shape** | Flat / nested env via `@env-spec` comments | Arbitrary JSON-shaped trees | Nested documents from JSON, YAML, or TOML |
| **Output / runtime form** | Resolved process env (or framework injection) | Text RX or binary RXB; read via Proxy / cursor | Layered binary file (header, schema, index, optional string table, data) |
| **Schema** | First-class (`.env.schema`, types, validation, IntelliSense) | None (schema-less; structural dedup only) | Optional semantic layer + `sensitive_paths`; `schema --agent-safe` / FFI export (no values) |
| **Partial / path access** | N/A (load full env set) | Native: O(1) array, O(log n) keyed lookup on indexes | Index + `Decoder::get` / `--path` (works with LZ4 data via decompress cache); optional `--index-maps-over` nested paths |
| **Inspect without full materialize** | N/A | Proxy / cursor over buffer | Offset `InspectNode` cursor (`inspect --tree` / `dump`); indexed root + wire walk |
| **Deduplication / size** | Not a serialization format | Strong: pointers, shared schemas, string chains | Opt-in structural dedup (`--dedup keys\|strings\|all`); LZ4 optional; size vs JSON still workload-dependent |
| **Integrity** | Leak scanning / runtime redaction (process-level) | Not a format concern | CRC64 in the file header |
| **Field encryption** | Secrets stay in backends; schema stays public | No | Yes (`protect`: `pbkdf2` and/or KMS envelope) |
| **Secret backends** | Broad plugin set (1Password, AWS, Azure, GCP, Vault, Infisical, Bitwarden, Doppler, K8s, …) | None | Resolvers at decode (`env`, Vault/OpenBao, AWS, Azure, GCP, Doppler, Infisical, Bitwarden, Akeyless, 1Password, Kubernetes, …) |
| **AI / agent focus** | Core product: schema for agents, secrets for humans; `varlock scan`; MCP docs | Not a focus | Agent-safe schema export, `bcs scan`, `AGENTS.md` / agents docs, stdio `bcs-mcp` — **config-file oriented**, not `.env` |
| **Leak / CI scanning** | `varlock scan` + hooks; log redaction in integrations | No | `bcs scan` (sources + `.bcs`); skips symlinks; kind-only findings (no matched secret text) |
| **Run / inject into process** | `varlock run` | N/A | `bcs run` (flatten env / `--json-env`; dry-run redacts sensitive) |
| **Framework integrations** | Deep JS ecosystem (Next.js, Vite, Astro, Expo, Cloudflare, …) | TypeScript/Node library + CLI | Rust core + C ABI + Python / TS / Swift / C# / Java bindings (no Next/Vite drop-ins) |
| **Ops CLI** | `load`, `run`, `scan`, init / plugins | `show`, `convert`, `inspect`, path segments | `encode` / `decode` / `validate` / `inspect` / `protect` / `reindex` / `scan` / `run` / `show` / `dump` / `schema` |
| **Implementation** | TypeScript (Bun monorepo) | TypeScript | Rust-first |
| **License** | MIT | MIT | MIT |

## Where they overlap

### BCS ≈ Varlock (secrets, agents, ops hygiene)

Both keep **secret values out of collaborative contracts**, resolve from external systems, offer **scan** + **run**, and expose agent-oriented schema/docs/MCP.

**They still diverge on the unit of work:**

- **Varlock** owns the *developer env workflow*: `.env.schema` as a single source of truth, multi-env loading, type coercion, log redaction in JS frameworks, password-manager UX aimed at day-to-day `.env` files.
- **BCS** owns the *packaged config artifact*: nested JSON/YAML/TOML → `.bcs` with CRC, optional index/dedup, field `protect`, secret refs, agent-safe *file* schema, and polyglot decode. `bcs run` injects from that file; it does not replace `@env-spec` / `.env.*` authoring.

Useful analogy: Varlock is *how teams author and inject env config safely*; BCS is *how ops ships a binary config container* that can share the same secret backends and agent-safe habits.

**Process injection (not `.env` files):** `bcs run` overlays flattened keys (or `--json-env`) onto a child process at the OS boundary — Node, Bun, Python, or any binary. See [env-injection.md](env-injection.md).

### BCS ≈ RX (indexed document access)

Both encode JSON-shaped trees and support **reading without materializing the whole document** (path get + inspect cursor in BCS; Proxy/cursor in RX). BCS also offers **opt-in structural dedup** and RX-inspired CLI DX (`show` segments, tree on TTY).

**They still diverge on goals and trade-offs:**

- **RX** optimizes for *very large*, write-once, sparsely read datasets (route tables, manifests). Heavy structural sharing, prefix chains, and zero-alloc Proxy reads are the product. Human-authored config is a **bad fit** per RX’s own docs. Text RX is copy-pasteable; RXB is the binary sibling.
- **BCS** optimizes for *configuration packaging*: integrity (CRC), compression, field-level protect, secret refs, validation, agent-safe export, and a multi-language ops CLI. Partial access and dedup are real features, not the reason to choose BCS over a data store. Size reduction vs JSON is **not** guaranteed unless `--dedup` / compression help your shape.

Useful analogy: RX is *no-SQL SQLite for JSON blobs*; BCS is a *config file container* with honest path/inspect tooling.

### Varlock ≉ RX

Almost no direct overlap. Comparing them only makes sense through BCS as the middle ground (secured packaged config + indexed reads).

## Strengths by project

| Project | Strong when you need… |
|---------|------------------------|
| **Varlock** | Typed `.env` schemas, multi-env DX, AI-safe collaboration on env vars, JS framework wiring, password-manager plugins as first-class product |
| **RX** | Huge JSON-like artifacts, microsecond sparse lookups, minimal heap, browser/edge embedding, copy-pasteable text encoding |
| **BCS** | Nested app/infra config as a shippable binary; CRC + protect + secret refs; agent-safe schema / scan / MCP; path get + offset inspect; Rust + polyglot bindings |

## When to choose each

- **Varlock** — Local and CI **env** management for JS (and CLI-wrapped) apps; agents that must see `.env` schema but never secrets; deep Next/Vite/Astro integrations.
- **RX** — Build or deploy produces a large JSON artifact; runtimes need random access without full parse; you do **not** need encryption, CRC, or secret resolution in the format.
- **BCS** — You author nested JSON/YAML/TOML, want a single binary file for runtime/ops, and care about integrity, field encryption, secret references, agent-safe file contracts, CI scan, and CLI/library access across languages.

## Can they work together?

Yes, in complementary layers:

1. **Varlock → process env** for developer and app bootstrap secrets / multi-env `.env` workflows.
2. **BCS** for packaged nested config deployed with the service (`protect` / secret refs / `bcs run` / agent-safe schema).
3. **RX** for large non-secret data blobs (manifests, catalogs) where BCS’s config-oriented layers would be overhead.

There is no requirement to use more than one; pick the layer that matches the artifact.

## What changed since the first comparison

Ideas adopted from Varlock/RX (see roadmaps above) that update this positioning:

| Area | Now in BCS |
|------|------------|
| Agent-safe contract | `sensitive_paths`, `schema --agent-safe`, FFI/bindings export, MCP `bcs_schema` |
| Leak / CI loop | `bcs scan`, `bcs run` (dry-run redacts), validate sensitive-plaintext policy |
| Path / inspect | Compress+path cache; `--index-maps-over`; offset `InspectNode`; `show` / `dump` |
| Size | Opt-in `--dedup`; still not “always smaller than JSON” |
| Secret DX | 1Password + Kubernetes resolvers (plus existing cloud/Vault set) |
| Agents | `AGENTS.md`, agents docs, `bcs-mcp` |

**Still not BCS:** `.env.schema` / `@env-spec`, Next.js/Vite drop-ins, RX text as a wire format, or language-binding Proxies for giant manifests.

## Verdict

Calling BCS “like Varlock” remains incomplete: Varlock is an **env schema and secret-injection platform**. BCS now shares agent-safe schemas, scan, run, and MCP — but for **packaged nested config files**, not `.env` multi-env workflows.

Calling BCS “like RX” remains incomplete: RX is a **performance-first embedded data store**. BCS now has stronger partial reads, inspect cursors, and optional dedup — but remains a **layered config container** with security and ops concerns RX deliberately omits.

**Short positioning:**

| Need | Prefer |
|------|--------|
| Env schema + AI-safe `.env` + JS frameworks | Varlock |
| Giant JSON, sparse in-place reads / text encoding | RX |
| Binary packaged config + protect / CRC / secret refs + agent-safe file ops | BCS |

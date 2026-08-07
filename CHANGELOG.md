# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- CI post-squash parity: Benchmark no longer hard-fails on shared-runner latency
  noise after merge (`BCS_GATE_FAIL_ON_LATENCY=0` in Actions; size still gates).
- Documentation workflow builds on PRs and skips GitHub Pages deploy when Pages
  is not enabled (was failing only on `push` to `main`).

### Changed

- Refresh `benchmarks/baseline/large.json` from GitHub Actions (ubuntu-latest)
  measurements so the Benchmark gate is not compared against a Darwin arm64
  baseline; slightly widen CI path-p95 thresholds for shared-runner noise.

## [0.1.0] - 2026-08-06

Initial **alpha** release of Binary Config Schema (BCS): a Rust-first binary
configuration container with optional indexing, compression, field-level
protection, secret references, language bindings, and an agent-safe MCP server.

Compatibility policy for `0.1.x` allows breaking format/API changes between
minors when required — see [docs/compatibility-policy.md](docs/compatibility-policy.md).

### Highlights

- Encode JSON / YAML / TOML into a checksummed `.bcs` container
- Indexed path reads, LZ4 data-layer compression, and opt-in structural dedup
- Password (`pbkdf2`) and KMS envelope (`kms`) field protection + secret refs
- Agent-safe schema / validate / scan / MCP tools that do not unlock secrets
- C ABI plus Python, TypeScript, Swift, C#, and Java wrappers (from source)
- Cross-platform CLI binaries via GitHub Releases; crates on crates.io

### Added

#### Format and core (`bcs-core`)

- Binary format with CRC64 integrity, optional index table, and resource limits
  for untrusted input (collection sizes, string/bytes lengths, LZ4 declared
  size, nesting depth ≤ 256)
- Memory-mapped decode (`Decoder::from_file_mmap`)
- Path lookup with cached decompress when the data layer is LZ4
- Structural dedup (`STRUCTURAL_DEDUP` / type tag `0x34`) with nested map indexes
- Offset-backed `InspectNode` cursor for tree inspect without full-document decode
- Shared leak-scan engine (`scan_path` / `ScanReport`)
- Schema `sensitive_paths`, agent-safe schema export, and golden compatibility tests
- Spec and measured benchmarks: [spec/format.md](spec/format.md),
  [docs/benchmarks.md](docs/benchmarks.md)

#### CLI (`bcs`)

- `encode`, `decode`, `inspect`, `validate`, `protect`, `reindex`, `benchmark`
- `scan` (secret-intent heuristics), `run` / `env` (process env injection),
  `show`, `dump`, `schema --agent-safe`
- Encode flags: `--compress-data`, `--dedup`, `--index-maps-over`,
  `--protect-paths` / `--sensitive-paths` (and file variants)
- Password via flag, `--*-password-env`, or interactive prompt
- Stream decode with mask / reveal / secret-ref resolve

#### Security and secrets

- Markers: `__bcs_sensitive_pbkdf2__:`, `__bcs_sensitive_kms__:`,
  `__bcs_secret_ref__:env:` / `__bcs_secret_ref__:secret:`
  (obsolete `__bcs_sensitive__:` rejected)
- Pluggable `SecretResolver` / `ResolverRegistry`; env provider built in
- Optional `bcs-secrets` backends (feature-gated): Vault/OpenBao, AWS, Azure,
  GCP, Doppler, Infisical, Akeyless, Bitwarden, 1Password, Kubernetes
- Native / `cmd` KMS wrappers for AWS, Azure, GCP, Vault Transit
- Injectable `CommandRunner` for `op` / `kubectl` with mock unit tests
- Default mask of protected values and secret refs unless explicitly unlocked
- Decode/show: `--redact-sensitive-plaintext` / `--fail-on-sensitive-plaintext`
- Security review: [docs/security-review.md](docs/security-review.md)

#### Agents and MCP

- `bcs-mcp` stdio server: `bcs_schema`, `bcs_inspect_meta`, `bcs_validate`,
  `bcs_scan`, `bcs_get_path`
- Agent docs: `AGENTS.md`, [docs/agents.md](docs/agents.md),
  [docs/agent-schema.md](docs/agent-schema.md), [docs/mcp.md](docs/mcp.md)

#### FFI and language bindings

- Stabilized C ABI (`bcs-ffi` / `bcs.h`): encode, decode, path, validate,
  protect, schema export, secret-resolve callback
- In-tree wrappers: Python, TypeScript/Node, Swift, C# (.NET 8), Java 22+ FFM
- Packaging / smoke scripts: `scripts/package-ffi.sh`,
  `scripts/run-binding-selftests.sh`
- Guide: [docs/bindings.md](docs/bindings.md)

#### Tooling and CI

- Workspace tests, clippy/fmt, binding self-tests (Python + TypeScript on Linux)
- Fuzz targets (`decode_bytes`, `get_path`) and weekly fuzz smoke
- `scripts/pre-release-check.sh`, `scripts/bench-gate.sh`,
  `scripts/record-benchmarks.sh`, `scripts/scan-ci-example.sh`
- Release workflow: GitHub Release CLI binaries + crates.io publish

### Security

- Fail closed on oversized allocations, corrupt indexed fields, and unsupported
  custom schema constraints
- `bcs scan` skips directory symlinks (no root escape)
- Password on argv emits a stderr warning favoring env / prompt

### Notes

- Language bindings and FFI natives are **alpha**; prefer the C header as the
  embedding contract. PyPI / npm packages are not part of this release
  (use repo + packaged natives).
- `--sensitive-paths` labels sensitivity in schema; use `--protect-paths` or
  secret refs for at-rest confidentiality.

[Unreleased]: https://github.com/trovante/bcs/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/trovante/bcs/releases/tag/v0.1.0

# BCS Compatibility Policy

This document defines compatibility expectations for the current project phase (`0.1.x`).

## Scope

Compatibility in this project covers:

- Binary file readability (`bcs-core` decoder)
- Binary file writability (`bcs-core` encoder)
- CLI behavior for stable flags and commands
- C ABI surface in `ffi/` (`bcs.h` / `libbcs_ffi`) used by language bindings
- Binding self-tests for wrappers that ship in-tree (Python, TypeScript, and others when tooling is available)

## Current Policy (`0.1.x`)

- The project is in alpha.
- Breaking changes are allowed between minor releases in `0.1.x` when required.
- Every intentional binary format change must:
  - update `spec/format.md`
  - update `CHANGELOG.md`
  - update or replace golden-file tests
- Intentional C ABI breaks must bump the documented FFI contract and update binding self-tests.

## Golden Compatibility Tests

The repository includes golden compatibility tests that assert deterministic encoding for canonical input payloads.

Current coverage includes:

- compact profile golden snapshot
- default profile golden snapshot (after canonical schema serialization)

If a golden test fails:

1. Verify whether the failure is intentional.
2. If intentional, regenerate golden snapshots and document the change.
3. If unintentional, treat as regression and fix encoder/decoder behavior.

## Decoder Expectations

- Files written by the current encoder must be readable by the current decoder.
- Checksum validation is mandatory.
- When `DATA_COMPRESSION` is set, path lookup decompresses the data layer once and caches it; offsets are relative to the logical (uncompressed) layer.
- When `STRUCTURAL_DEDUP` (`0x0008`) is set, readers must load the string table and resolve type tag `0x34`; unknown readers must fail closed.
- Resource limits apply to untrusted input (collection sizes, string/bytes lengths, LZ4 declared size, and nesting depth).

## CLI Expectations

- Existing documented flags should remain stable within patch releases.
- Security-related flags must keep safe defaults (masked output when decode password is not provided; secret refs masked unless resolve is opted in).

## FFI and language bindings

- The C ABI is the supported embedding contract for non-Rust runtimes.
- In-tree bindings (Python, TypeScript, Swift, C#, Java) are alpha and track the FFI; they are exercised by `scripts/run-binding-selftests.sh` and by the CI `binding-selftests` job (Python + TypeScript on Linux).
- Binding APIs may change in `0.1.x` when the FFI or core semantics change; prefer the C header as the source of truth for symbol names and ownership (`bcs_free_buffer` / `bcs_free_string`).

## Sensitive-field payload schemes

Protected markers use **named schemes with distinct prefixes** (no version tags):

- `__bcs_sensitive_pbkdf2__:` — PBKDF2-HMAC-SHA256 + AES-256-GCM (password KDF)
- `__bcs_sensitive_kms__:` — AES-256-GCM under a random DEK; DEK wrapped via host/native KMS

Both may appear in the same file. See `docs/identity.md`. The obsolete
`__bcs_sensitive__:` prefix is rejected.

## Out of Scope

- Guaranteed ABI stability of individual high-level binding wrappers across `0.1.x` patch releases (FFI is the stable-ish contract; wrappers may adapt).
- Backward compatibility guarantees for pre-`0.1.x` snapshots.

## Policy Evolution

When the project reaches `1.0`, this policy will be tightened to include stronger backward-compatibility guarantees and explicit deprecation windows for both the binary format and the C ABI.

# ADR: Security exposure remediations (decode/show/FFI docs)

**Status:** Accepted  
**Date:** 2026-08-06  
**Scope:** Follow-ups from the product security evaluation (data exposure on CLI/MCP/FFI)

## Context

Safe-by-default surfaces mask protect markers and secret refs, but:

1. `decode --path` only masked a leaf protect marker, not nested markers under a subtree.
2. Schema `sensitive_paths` did not affect decode/show output (validate warn/fail only).
3. `--password` on argv remains a footgun (history / `ps`).
4. FFI reveal-with-password was not called out as a distinct trust boundary from MCP.

## Decision

1. **Align path decode with full decode** — always run recursive `mask_all` /
   `reveal_all_ex` and secret-ref mask/resolve on the subtree.
2. **Opt-in plaintext policy on decode/show** — `--redact-sensitive-plaintext`
   replaces sensitive plaintext with `[SENSITIVE]`; `--fail-on-sensitive-plaintext`
   refuses to print. Both require an embedded schema. Core helpers:
   `find_sensitive_plaintext_under` / `redact_sensitive_plaintext_under`.
   After mask, display placeholders (`[PROTECTED]`, `[SECRET_REF]`, `[SENSITIVE]`)
   are treated as non-plaintext so protect workflows do not false-fail.
3. **Warn on argv passwords** — `warn_password_on_argv` for decode/run/encode/protect
   paths that accept a direct password flag.
4. **Document FFI ≠ MCP** — bindings guide states that FFI is a host-trusted API;
   MCP never accepts unlock credentials.

## Consequences

- Defaults unchanged for unmarked plaintext (no surprise redaction).
- CI and agent workflows can harden decode/show without changing validate-only gates.
- Operators still responsible for encrypting or referencing secrets at encode time.

## References

- [security-review.md](../security-review.md)
- [cli-security.md](../cli-security.md)
- [bindings.md](../bindings.md)
- [security-review-followups.md](security-review-followups.md) (F3)

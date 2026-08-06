# ADR: Security review of scan / run / agent-safe / MCP (follow-ups F3)

**Status:** Accepted  
**Date:** 2026-08-05  
**Scope:** Surfaces listed in [roadmap-followups-inspect-gaps.md](../roadmap-followups-inspect-gaps.md) F3

## Method

Manual checklist review of current tree (security-review subagent could not compute an uncommitted diff). Focus: secret leakage to stdout/MCP, argv injection, directory scan escape.

## Findings

| Severity | Location | Finding | Disposition |
|----------|----------|---------|-------------|
| should-fix | `core/src/scan.rs` `scan_dir` | Directory walk followed symlinks (could escape intended roots) | **Fixed** — skip symlink entries |
| accept | `core/src/scan.rs` findings | Messages use pattern *kind* only (e.g. `aws_access_key_id`), not matched substrings | Accept — policy: never echo matched secrets |
| accept | `schema` / `find_sensitive_plaintext` | Messages name the path, not the plaintext value | Accept |
| accept | `cli/.../run.rs` dry-run | Sensitive keys print `[REDACTED]`; JSON env dry-run redacted | Accept |
| accept | MCP `bcs_get_path` / FFI path | Always mask protect/secret markers | Accept |
| accept | `op` / `kubectl` resolvers | Argv via `CommandRunner`; no `sh -c`; tests use fakes | Accept ([secret-cli-runner.md](secret-cli-runner.md)) |
| accept | MCP / `schema --agent-safe` | Export has no data-layer values (covered by tests) | Accept |

## Blockers

None open after symlink skip.

## Checklist sign-off

- [x] Dry-run / MCP / `get_path` never print revealed secrets by default
- [x] Agent-safe schema contains no planted plaintext values in tests
- [x] `scan` JSON report does not include full matched secret strings
- [x] Secret CLI resolvers use argument vectors, not `sh -c`
- [x] `scan` on directories does not follow symlinks

## Release

Add a line to [.github/release-checklist.md](../../.github/release-checklist.md) referencing this ADR for releases that ship scan/run/MCP.

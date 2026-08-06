# ADR: Injectable CommandRunner for CLI secret backends

**Status:** Accepted  
**Date:** 2026-08-05  
**Context:** F2 in [roadmap-followups-inspect-gaps.md](../roadmap-followups-inspect-gaps.md)

## Decision

`op` (1Password) and `kubectl` (Kubernetes) secret resolvers take an injectable `CommandRunner` (`secrets/src/cmd_runner.rs`). Production uses `StdCommandRunner` (`std::process::Command` with argv only). Tests supply fakes that return `CommandOutput` without spawning processes.

## Consequences

- CI can enable `onepassword` / `kubernetes` features for unit tests without installing CLIs or clusters.
- No shell (`sh -c`) — arguments are passed as a vector.
- In-cluster HTTP K8s client remains a future option; kubectl MVP stays.

## Alternatives rejected

- PATH shim scripts per test — fragile on Windows and harder to assert argv.
- Always calling real binaries in CI — non-hermetic and slow.

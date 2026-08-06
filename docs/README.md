# Documentation

Guides and references for Binary Config Schema (BCS).

## Start here

| Doc | Description |
|-----|-------------|
| [CLI Usage](cli-usage.md) | Encode, decode, validate, inspect, benchmark, reindex, path queries |
| [Environment injection](env-injection.md) | `bcs run` / `bcs env`: inject config into process env (no `.env` files) |
| [Protecting Fields & Secrets](cli-security.md) | Password/KMS protect, secret-ref recipes, and security-surfaces diagram |
| [Agents](agents.md) | Agent-safe workflows (no secret values) + security-surfaces diagram |
| [Root README — Security surfaces](../README.md#security-surfaces) | Overview diagram: safe / values / unlock lanes |
| [Agent-safe schema](agent-schema.md) | JSON contract for `schema --agent-safe` |
| [MCP Server](mcp.md) | stdio MCP tools (`bcs-mcp`) for agents |
| [Examples](examples.md) | Library and workflow examples |
| [API Reference](api-reference.md) | Rust API |

## Format & design

| Doc | Description |
|-----|-------------|
| [Format Specification](../spec/format.md) | Binary layout (source of truth) |
| [BCS vs Protobuf vs MessagePack](comparison-protobuf-msgpack.md) | Positioning against common binary formats |
| [BCS vs Varlock vs RX](comparison-varlock-rx.md) | Positioning against env/schema tooling and random-access JSON stores (updated after roadmap) |
| [Roadmap: Varlock & RX ideas](roadmap-varlock-rx-ideas.md) | Implemented plan of improvements from that comparison |
| [Follow-ups: inspect cursor & gaps](roadmap-followups-inspect-gaps.md) | Offset inspect AST, remaining bindings, mock providers, security review |
| [ADR: secret CLI runner](adr/secret-cli-runner.md) | Injectable `op` / `kubectl` runner for tests |
| [ADR: inspect cursor](adr/inspect-cursor.md) | Offset-based inspect AST (implemented) |
| [ADR: security review follow-ups](adr/security-review-followups.md) | F3 scan/run/MCP review + symlink fix |
| [Compatibility Policy](compatibility-policy.md) | Versioning and compatibility |
| [CLI JSON Output Contract](cli-json-output.md) | Machine-readable CLI output |

## Security & identity

| Doc | Description |
|-----|-------------|
| [Security review](security-review.md) | Data-exposure evaluation, remediations, operator checklist |
| [SECURITY.md](../SECURITY.md) | Vulnerability reporting policy |
| [ADR: exposure remediations](adr/security-exposure-remediations.md) | decode/show redact/fail, path mask align, argv warning, FFI≠MCP |
| [ADR: security review follow-ups](adr/security-review-followups.md) | F3 scan/run/MCP review + symlink fix |
| [Identity & Secret Strategy](identity.md) | `pbkdf2` / `kms` prefixes, native KMS, OIDC/IAM |
| [Secret Providers](secrets.md) | Vault, AWS, Azure, GCP, and other resolvers |

## Performance

| Doc | Description |
|-----|-------------|
| [Measured Benchmarks](benchmarks.md) | Reproducible size and latency numbers |
| [Performance Gates Policy](performance-gates.md) | Gate methodology and thresholds |

## Bindings & development

| Doc | Description |
|-----|-------------|
| [Language Bindings Guide](bindings.md) | FFI natives + Python/TS/Swift/C#/Java/C |
| [Development](development.md) | Build, test, lint, pre-release, bench gate |
| [Contributing](../CONTRIBUTING.md) | Contribution workflow |
| [FFI README](../ffi/README.md) | C ABI packaging |
| [Bindings README](../bindings/README.md) | Status matrix and self-tests |

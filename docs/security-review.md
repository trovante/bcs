# Security review: data exposure remediations (2026-08)

Product security review of CLI / MCP / FFI data-exposure surfaces, plus remediations
shipped after the evaluation captured in the security-surfaces diagram
([README](../README.md#security-surfaces), [agents.md](agents.md), [cli-security.md](cli-security.md)).

Related: [ADR: remediations](adr/security-exposure-remediations.md),
[ADR: F3 follow-ups](adr/security-review-followups.md).

## Verdict

Mask defaults for protect markers and secret refs are sound. Confidentiality still
requires `--protect-paths` or `__bcs_secret_ref__` — `--sensitive-paths` is a
**label only**. Unmarked plaintext remains readable on value-printing surfaces unless
operators opt into redact/fail flags.

## Surfaces (summary)

| Lane | Surfaces | Secrets |
|------|----------|---------|
| Safe by default | `schema --agent-safe`, `validate`, `scan`, inspect meta, MCP tools | Never unlocked |
| Can print values | `decode`, `show`, `inspect --tree`, FFI decode | Unprotected plaintext; markers masked |
| Explicit unlock | `--password` / `--unwrap-kms` / `--resolve-secrets` / `env --allow-sensitive` / `bcs run` | Revealed / injected |

## Remediations implemented

| Item | Change |
|------|--------|
| Nested protect on `decode --path` | Partial decode now runs recursive `mask_all` / `reveal_all_ex` like full decode |
| Sensitive plaintext on decode/show | `--redact-sensitive-plaintext` → `[SENSITIVE]`; `--fail-on-sensitive-plaintext` refuses output |
| Password on argv | stderr warning pointing operators to `--password-env` / prompt |
| FFI vs MCP trust boundary | Documented in [bindings.md](bindings.md) |

## Operator checklist

- Prefer secret refs + KMS / workload identity in production ([identity.md](identity.md))
- Use `--protect-paths`, not only `--sensitive-paths`, for at-rest encryption
- CI: `bcs validate --fail-on-sensitive-plaintext` and `bcs scan`
- Agents: MCP / `schema --agent-safe` / validate / scan only
- Prefer `--password-env` or interactive prompt over `--password`
- Before live inject: `bcs run --dry-run`; avoid `env --allow-sensitive`
- Decode/show with schema-marked plaintext: add `--redact-sensitive-plaintext` or `--fail-on-sensitive-plaintext`

## Residual risks (accepted)

- Unprotected plaintext still prints by default on decode/show (opt-in redact/fail)
- KMS / secret-ref locator metadata remains visible in the file
- Weak PBKDF2 passwords remain offline-attackable
- FFI hosts that pass passwords/callbacks get raw secrets (by design; higher trust than MCP)

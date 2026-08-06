# Security Policy

## Supported versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | Yes (alpha) |

During `0.1.x`, the binary format and APIs may still change. Prefer the latest
patch on the current minor when reporting issues.

## Reporting a vulnerability

Please **do not** open a public GitHub issue for security vulnerabilities.

Report privately via one of:

- GitHub Security Advisories: **Security → Report a vulnerability** on
  [trovante/bcs](https://github.com/trovante/bcs)
- Email the maintainers if GitHub private reporting is unavailable (see the
  repository owner / org contact)

Include:

- BCS version / commit
- Affected surface (CLI command, MCP tool, FFI, decoder of untrusted input, etc.)
- Reproduction steps or a minimal PoC
- Impact assessment (confidentiality, integrity, availability)

We aim to acknowledge reports within a few business days.

## Operator guidance

Safe-by-default surfaces (`schema --agent-safe`, `validate`, `scan`, MCP tools)
do not unlock secret values. Commands that print values (`decode`, `show`, FFI
decode) can still expose unprotected plaintext; protected fields and secret refs
require explicit unlock flags.

See [docs/security-review.md](docs/security-review.md) and
[docs/cli-security.md](docs/cli-security.md).

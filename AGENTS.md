# Agent guide for BCS

Binary Config Schema (BCS) packages nested JSON/YAML/TOML into a shippable `.bcs` file with optional schema, index, protect, and secret references.

## Do

- Prefer `bcs schema --agent-safe <file.bcs>` (or MCP tool `bcs_schema`) to learn shape and sensitivity **without values**.
- Prefer `bcs inspect` / `bcs validate` (or MCP `bcs_inspect_meta` / `bcs_validate`) for structure and policy checks.
- Use `bcs scan` / MCP `bcs_scan` before shipping configs that may contain secrets.
- Encode with `--protect-paths` or `--sensitive-paths` so agents see which fields are sensitive.
- When the BCS MCP server is configured, prefer its tools over decoding full files in chat.

## Do not

- Ask humans for protect passwords or KMS unwrap secrets in chat logs.
- Print decoded values for sensitive paths unless the operator explicitly unlocks them with CLI flags.
- Treat `AISemanticTag.sensitivity` as the source of truth; use schema `sensitive_paths`.

## Common workflows

```bash
# Pack config and mark/encrypt secrets
bcs encode config.json -o config.bcs \
  --protect-paths database.password,api.key \
  --protect-password-env BCS_PROTECT_PASSWORD

# Inject into any process (Node/Bun/Python/…) — no .env files
bcs run config.bcs --resolve-secrets -- ./my-app
# See docs/env-injection.md

# Agent-safe contract (no values)
bcs schema --agent-safe config.bcs

# Validate (warns on sensitive plaintext; fail with flag)
bcs validate config.bcs
bcs validate config.bcs --fail-on-sensitive-plaintext

# Path read without full-document decode when index is present
bcs decode config.bcs --path database.host
```

See [docs/agents.md](docs/agents.md), [docs/agent-schema.md](docs/agent-schema.md), and [docs/mcp.md](docs/mcp.md).

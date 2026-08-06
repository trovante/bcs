# BCS MCP Server

Agent-facing [Model Context Protocol](https://modelcontextprotocol.io/) server for Binary Config Schema.

## Summary

| Item | Value |
|------|--------|
| Binary | `bcs-mcp` (`mcp/` crate) |
| Transport | stdio (`rmcp`) |
| Backend | `bcs-core` (shared scan engine with CLI) |
| Secrets | Never unlock; path reads always masked |

ADR: [adr/mcp-server.md](adr/mcp-server.md).

## Install / run

```bash
cargo build -p bcs-mcp --release
```

Cursor `mcp.json` / Claude Desktop:

```json
{
  "mcpServers": {
    "bcs": {
      "command": "/absolute/path/to/target/release/bcs-mcp"
    }
  }
}
```

## Tools

### `bcs_schema`

- **Input:** `{ "path": "<file.bcs>" }`
- **Output:** agent-safe schema JSON ([agent-schema.md](agent-schema.md))

### `bcs_inspect_meta`

- **Input:** `{ "path": "<file.bcs>" }`
- **Output:** version, sizes, flags, schema summary, `sensitive_paths` names, index stats — **no values**

### `bcs_validate`

- **Input:** `{ "path": "<file.bcs>", "fail_on_sensitive_plaintext": false }`
- **Output:** `{ ok, errors, warnings, ... }` — same policy as `bcs validate`

### `bcs_scan`

- **Input:** `{ "path": "<file-or-dir>", "fail_on": "finding" }` (`finding` \| `warn`)
- **Output:** same shape as `bcs scan --json`

### `bcs_get_path`

- **Input:** `{ "path": "<file.bcs>", "query": "database.host" }`
- **Output:** JSON value with protect markers → `[PROTECTED]` and secret refs → `[SECRET_REF]`

## Security guarantees

1. No tool accepts a protect password or KMS unwrap secret.
2. Agent-safe schema and inspect never emit data-layer values.
3. `bcs_get_path` always runs `mask_sensitive_fields` before returning.

## Related

- [agents.md](agents.md)
- [AGENTS.md](../AGENTS.md)
- [cli-security.md](cli-security.md)
- [roadmap-varlock-rx-ideas.md](roadmap-varlock-rx-ideas.md) (P2-B)

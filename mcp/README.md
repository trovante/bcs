# BCS MCP Server

stdio MCP server exposing agent-safe BCS tools. Backed by `bcs-core` (no CLI subprocess). Never accepts protect passwords or reveals protected plaintext.

## Build

```bash
cargo build -p bcs-mcp --release
# binary: target/release/bcs-mcp
```

## Cursor / Claude Desktop config

```json
{
  "mcpServers": {
    "bcs": {
      "command": "/absolute/path/to/bcs/target/release/bcs-mcp"
    }
  }
}
```

Or during development:

```json
{
  "mcpServers": {
    "bcs": {
      "command": "cargo",
      "args": ["run", "-q", "-p", "bcs-mcp"],
      "cwd": "/absolute/path/to/bcs"
    }
  }
}
```

Logs go to **stderr**; stdout is reserved for MCP JSON-RPC.

## Tools

| Tool | Purpose |
|------|---------|
| `bcs_schema` | Agent-safe schema JSON (no data values) |
| `bcs_inspect_meta` | Header / schema / index summary; sensitive path **names** only |
| `bcs_validate` | Schema validation + sensitive-plaintext policy |
| `bcs_scan` | Leak / sensitive-plaintext scan (same JSON as `bcs scan --json`) |
| `bcs_get_path` | Partial path read; protect markers and secret refs always masked |

## Security

- Do not pass unlock passwords to this server (there is no parameter for them).
- Prefer `bcs_schema` / `bcs_inspect_meta` / `bcs_validate` / `bcs_scan` before decoding values.
- See [docs/mcp.md](../docs/mcp.md) and [AGENTS.md](../AGENTS.md).

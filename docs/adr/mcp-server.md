# ADR: BCS MCP server

**Status:** Implemented  
**Date:** 2026-08-05  
**Context:** P2-B follow-up in [roadmap-varlock-rx-ideas.md](../roadmap-varlock-rx-ideas.md)

## Decision

Ship a workspace crate `mcp/` (`bcs-mcp`) as a **stdio** MCP server using the official Rust SDK (`rmcp`):

- Tools call **`bcs-core`** directly (not the CLI subprocess).
- Leak scanning uses the shared `bcs_core::scan` engine (CLI is a thin wrapper).
- v1 tools: `bcs_schema`, `bcs_inspect_meta`, `bcs_validate`, `bcs_scan`, `bcs_get_path`.
- Path reads are always masked; no password / KMS unlock parameters.

## Consequences

- Agents (Cursor, etc.) can configure a single local binary without installing a separate language runtime.
- HTTP / Streamable HTTP transport deferred.
- Encode / protect / `run` stay CLI-only for v1 (higher blast radius).

## Non-goals

- Remote multi-tenant MCP hosting
- Revealing protected values through MCP
- Full docs-site resources (optional later: `bcs://agents`)

//! `bcs-mcp` — stdio MCP server for agent-safe BCS tools.

mod ops;
mod server;

use anyhow::Result;
use rmcp::{transport::stdio, ServiceExt};
use server::BcsMcp;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Log to stderr so stdout stays clean for MCP JSON-RPC.
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("bcs_mcp=info".parse()?))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let service = BcsMcp::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

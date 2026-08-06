//! MCP tool wiring for BCS agent-safe operations.

use crate::ops;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ServerHandler,
};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct BcsMcp {
    tool_router: ToolRouter<Self>,
}

impl BcsMcp {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

impl Default for BcsMcp {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PathArg {
    /// Absolute or relative path to a `.bcs` file
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ValidateArg {
    /// Path to a `.bcs` file
    pub path: String,
    /// When true, sensitive plaintext fails validation (default: false / warn)
    #[serde(default)]
    pub fail_on_sensitive_plaintext: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ScanArg {
    /// File or directory to scan (JSON/YAML/TOML/`.bcs`)
    pub path: String,
    /// Fail policy: `finding` (default) or `warn`
    #[serde(default = "default_fail_on")]
    pub fail_on: String,
}

fn default_fail_on() -> String {
    "finding".into()
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetPathArg {
    /// Path to a `.bcs` file
    pub path: String,
    /// Dot/bracket path query (e.g. `database.host`)
    pub query: String,
}

fn tool_err(msg: String) -> String {
    msg
}

#[tool_router]
impl BcsMcp {
    #[tool(
        name = "bcs_schema",
        description = "Export agent-safe schema JSON from a .bcs file (paths, types, sensitive flags; never data values)."
    )]
    fn bcs_schema(&self, Parameters(arg): Parameters<PathArg>) -> Result<String, String> {
        ops::schema_agent_safe(&PathBuf::from(arg.path)).map_err(tool_err)
    }

    #[tool(
        name = "bcs_inspect_meta",
        description = "Inspect BCS header, schema summary, and index stats without returning data values. Lists sensitive path names only."
    )]
    fn bcs_inspect_meta(&self, Parameters(arg): Parameters<PathArg>) -> Result<String, String> {
        let v = ops::inspect_meta(&PathBuf::from(arg.path)).map_err(tool_err)?;
        serde_json::to_string_pretty(&v).map_err(|e| e.to_string())
    }

    #[tool(
        name = "bcs_validate",
        description = "Validate a .bcs file against its embedded schema. Sensitive plaintext warns by default; set fail_on_sensitive_plaintext to fail."
    )]
    fn bcs_validate(&self, Parameters(arg): Parameters<ValidateArg>) -> Result<String, String> {
        let v = ops::validate(&PathBuf::from(arg.path), arg.fail_on_sensitive_plaintext)
            .map_err(tool_err)?;
        serde_json::to_string_pretty(&v).map_err(|e| e.to_string())
    }

    #[tool(
        name = "bcs_scan",
        description = "Scan a file or directory for leaked secrets and sensitive plaintext (same report as `bcs scan --json`)."
    )]
    fn bcs_scan(&self, Parameters(arg): Parameters<ScanArg>) -> Result<String, String> {
        let v = ops::scan(&PathBuf::from(arg.path), &arg.fail_on).map_err(tool_err)?;
        serde_json::to_string_pretty(&v).map_err(|e| e.to_string())
    }

    #[tool(
        name = "bcs_get_path",
        description = "Partial path read from a .bcs file. Protect markers and secret refs are always masked; this tool never accepts a password."
    )]
    fn bcs_get_path(&self, Parameters(arg): Parameters<GetPathArg>) -> Result<String, String> {
        let v = ops::get_path_masked(&PathBuf::from(arg.path), &arg.query).map_err(tool_err)?;
        serde_json::to_string_pretty(&v).map_err(|e| e.to_string())
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for BcsMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "BCS MCP: agent-safe tools for Binary Config Schema. \
                 Prefer bcs_schema / bcs_inspect_meta / bcs_validate / bcs_scan / bcs_get_path. \
                 Never ask humans for protect passwords or KMS unwrap secrets. \
                 Path reads are always masked.",
            )
    }
}

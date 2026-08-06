//! Injectable process runner for CLI-backed secret providers (`op`, `kubectl`).
//!
//! Production uses [`StdCommandRunner`]; tests inject a fake that never shells out.

use bcs_core::{BCSError, Result};
use std::process::Command;
use std::sync::Arc;

/// Result of running an argv vector (no shell).
#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub success: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl CommandOutput {
    pub fn ok_stdout(stdout: impl Into<Vec<u8>>) -> Self {
        Self {
            success: true,
            stdout: stdout.into(),
            stderr: Vec::new(),
        }
    }

    pub fn fail_stderr(stderr: impl Into<Vec<u8>>) -> Self {
        Self {
            success: false,
            stdout: Vec::new(),
            stderr: stderr.into(),
        }
    }
}

/// Runs an argv vector and returns process output (no shell).
pub trait CommandRunner: Send + Sync {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput>;
}

/// Default runner using [`std::process::Command`].
#[derive(Debug, Default, Clone, Copy)]
pub struct StdCommandRunner;

impl CommandRunner for StdCommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput> {
        let output = Command::new(program).args(args).output().map_err(|e| {
            BCSError::Decoding(format!(
                "Failed to run `{}` (is it installed?): {}",
                program, e
            ))
        })?;
        Ok(CommandOutput {
            success: output.status.success(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

/// Shared runner handle used by CLI-backed resolvers.
pub type SharedRunner = Arc<dyn CommandRunner>;

pub fn default_runner() -> SharedRunner {
    Arc::new(StdCommandRunner)
}

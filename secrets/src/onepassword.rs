//! 1Password secret resolver via `op` CLI (Connect optional later).
//!
//! Feature: `onepassword`
//!
//! Locators: `op://vault/item/field` or `vault/item/field`
//!
//! Prefer short-lived tokens; never log resolved secrets.

use crate::cmd_runner::{default_runner, CommandRunner, SharedRunner};
use bcs_core::secret_resolver::SecretResolver;
use bcs_core::{BCSError, Result};
use std::sync::Arc;

/// Resolves `op:` / `onepassword:` secret references via the 1Password CLI.
#[derive(Clone)]
pub struct OnePasswordSecretResolver {
    runner: SharedRunner,
}

impl std::fmt::Debug for OnePasswordSecretResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OnePasswordSecretResolver").finish()
    }
}

impl Default for OnePasswordSecretResolver {
    fn default() -> Self {
        Self {
            runner: default_runner(),
        }
    }
}

impl OnePasswordSecretResolver {
    pub fn from_env() -> Result<Self> {
        Ok(Self::default())
    }

    /// Inject a custom [`CommandRunner`] (tests / alternate backends).
    pub fn with_runner(runner: Arc<dyn CommandRunner>) -> Self {
        Self { runner }
    }
}

impl SecretResolver for OnePasswordSecretResolver {
    fn resolve(&self, scheme: &str, locator: &str) -> Result<String> {
        if scheme != "op" && scheme != "onepassword" {
            return Err(BCSError::Decoding(format!(
                "OnePasswordSecretResolver does not handle scheme '{}'",
                scheme
            )));
        }

        let op_ref = if locator.starts_with("op://") {
            locator.to_string()
        } else {
            format!("op://{}", locator)
        };
        let output = self.runner.run("op", &["read", &op_ref])?;
        if !output.success {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BCSError::Decoding(format!(
                "`op read` failed: {}",
                stderr.trim()
            )));
        }
        let value = String::from_utf8(output.stdout)
            .map_err(|e| BCSError::Decoding(format!("op read returned non-UTF8: {}", e)))?;
        Ok(value.trim_end_matches('\n').to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd_runner::CommandOutput;

    struct FakeOp {
        stdout: String,
        success: bool,
        stderr: String,
    }

    impl CommandRunner for FakeOp {
        fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput> {
            assert_eq!(program, "op");
            assert_eq!(args.first().copied(), Some("read"));
            if self.success {
                Ok(CommandOutput::ok_stdout(format!("{}\n", self.stdout)))
            } else {
                Ok(CommandOutput::fail_stderr(self.stderr.clone()))
            }
        }
    }

    #[test]
    fn rejects_wrong_scheme() {
        let r = OnePasswordSecretResolver::default();
        assert!(r.resolve("env", "x").is_err());
    }

    #[test]
    fn resolves_op_ref_via_runner() {
        let r = OnePasswordSecretResolver::with_runner(Arc::new(FakeOp {
            stdout: "s3cret-from-op".into(),
            success: true,
            stderr: String::new(),
        }));
        let v = r.resolve("op", "vault/item/field").unwrap();
        assert_eq!(v, "s3cret-from-op");
    }

    #[test]
    fn accepts_full_op_uri() {
        let r = OnePasswordSecretResolver::with_runner(Arc::new(FakeOp {
            stdout: "ok".into(),
            success: true,
            stderr: String::new(),
        }));
        assert_eq!(r.resolve("onepassword", "op://v/i/f").unwrap(), "ok");
    }

    #[test]
    fn surfaces_cli_failure() {
        let r = OnePasswordSecretResolver::with_runner(Arc::new(FakeOp {
            stdout: String::new(),
            success: false,
            stderr: "not signed in".into(),
        }));
        let err = r.resolve("op", "v/i/f").unwrap_err().to_string();
        assert!(err.contains("not signed in"), "{err}");
    }
}

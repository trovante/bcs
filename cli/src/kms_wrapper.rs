//! External-command and native `KeyWrapper` selection for the CLI.

use anyhow::{Context, Result};
use base64::Engine;
use bcs_core::error::BCSError;
use bcs_core::security::KeyWrapper;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Arc;

const DEK_LEN: usize = 32;

/// Wraps/unwraps DEKs by invoking shell commands from the environment.
///
/// - `BCS_KMS_WRAP_CMD` / `BCS_KMS_UNWRAP_CMD`: commands run via `sh -c`
///   (Unix) or `cmd /C` (Windows)
/// - stdin: base64(DEK or wrapped DEK) + newline
/// - stdout: base64(result)
/// - env: `BCS_KMS_PROVIDER`, `BCS_KMS_KEY`
pub struct CommandKeyWrapper {
    wrap_cmd: String,
    unwrap_cmd: String,
}

impl CommandKeyWrapper {
    pub fn from_env() -> Result<Self> {
        let wrap_cmd = std::env::var("BCS_KMS_WRAP_CMD").map_err(|_| {
            anyhow::anyhow!(
                "BCS_KMS_WRAP_CMD is required for --kms-provider cmd (or use a native provider: {})",
                bcs_secrets::available_kms_providers().join(", ")
            )
        })?;
        let unwrap_cmd = std::env::var("BCS_KMS_UNWRAP_CMD").map_err(|_| {
            anyhow::anyhow!(
                "BCS_KMS_UNWRAP_CMD is required for --kms-provider cmd (or use a native provider: {})",
                bcs_secrets::available_kms_providers().join(", ")
            )
        })?;
        if wrap_cmd.trim().is_empty() || unwrap_cmd.trim().is_empty() {
            anyhow::bail!("BCS_KMS_WRAP_CMD / BCS_KMS_UNWRAP_CMD must be non-empty");
        }
        Ok(Self {
            wrap_cmd,
            unwrap_cmd,
        })
    }

    pub fn from_env_unwrap_only() -> Result<Self> {
        let unwrap_cmd = std::env::var("BCS_KMS_UNWRAP_CMD").map_err(|_| {
            anyhow::anyhow!(
                "BCS_KMS_UNWRAP_CMD is required for --kms-provider cmd unwrap (or pass a native --kms-provider)"
            )
        })?;
        if unwrap_cmd.trim().is_empty() {
            anyhow::bail!("BCS_KMS_UNWRAP_CMD must be non-empty");
        }
        Ok(Self {
            wrap_cmd: String::new(),
            unwrap_cmd,
        })
    }

    fn run(cmd: &str, provider: &str, kek_locator: &str, input: &[u8]) -> Result<Vec<u8>> {
        let input_b64 = base64::engine::general_purpose::STANDARD.encode(input);
        #[cfg(windows)]
        let mut child = Command::new("cmd")
            .arg("/C")
            .arg(cmd)
            .env("BCS_KMS_PROVIDER", provider)
            .env("BCS_KMS_KEY", kek_locator)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("Failed to spawn KMS command: {}", cmd))?;
        #[cfg(not(windows))]
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .env("BCS_KMS_PROVIDER", provider)
            .env("BCS_KMS_KEY", kek_locator)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("Failed to spawn KMS command: {}", cmd))?;

        {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| anyhow::anyhow!("Failed to open stdin for KMS command"))?;
            stdin
                .write_all(input_b64.as_bytes())
                .context("Failed to write to KMS command stdin")?;
            stdin
                .write_all(b"\n")
                .context("Failed to write newline to KMS command stdin")?;
        }

        let output = child
            .wait_with_output()
            .context("Failed to wait for KMS command")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "KMS command failed with status {}: {}",
                output.status,
                stderr.trim()
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let trimmed = stdout.trim();
        base64::engine::general_purpose::STANDARD
            .decode(trimmed)
            .with_context(|| "KMS command stdout was not valid base64")
    }
}

impl KeyWrapper for CommandKeyWrapper {
    fn wrap(&self, provider: &str, kek_locator: &str, dek: &[u8]) -> bcs_core::Result<Vec<u8>> {
        if self.wrap_cmd.is_empty() {
            return Err(BCSError::Encoding(
                "KMS wrap command is not configured".to_string(),
            ));
        }
        Self::run(&self.wrap_cmd, provider, kek_locator, dek)
            .map_err(|e| BCSError::Encoding(e.to_string()))
    }

    fn unwrap(
        &self,
        provider: &str,
        kek_locator: &str,
        wrapped_dek: &[u8],
    ) -> bcs_core::Result<[u8; DEK_LEN]> {
        if self.unwrap_cmd.is_empty() {
            return Err(BCSError::Decoding(
                "KMS unwrap command is not configured".to_string(),
            ));
        }
        let bytes = Self::run(&self.unwrap_cmd, provider, kek_locator, wrapped_dek)
            .map_err(|e| BCSError::Decoding(e.to_string()))?;
        if bytes.len() != DEK_LEN {
            return Err(BCSError::Decoding(format!(
                "KMS unwrap command returned {} bytes, expected {}",
                bytes.len(),
                DEK_LEN
            )));
        }
        let mut out = [0u8; DEK_LEN];
        out.copy_from_slice(&bytes);
        Ok(out)
    }
}

/// Select a wrap/unwrap backend for protect (`cmd` or native aws/azure/gcp/vault).
pub fn resolve_key_wrapper(provider: &str) -> Result<Arc<dyn KeyWrapper + Send + Sync>> {
    match provider {
        "cmd" => Ok(Arc::new(CommandKeyWrapper::from_env()?)),
        other => bcs_secrets::key_wrapper_for_provider(other)
            .map_err(anyhow::Error::msg)
            .with_context(|| {
                format!(
                    "Failed to initialize KMS provider '{}' (available: cmd, {})",
                    other,
                    bcs_secrets::available_kms_providers().join(", ")
                )
            }),
    }
}

/// Select unwrap backend for decode.
///
/// When `provider` is set, use that backend. Otherwise register all native
/// providers that can be constructed from the environment, plus `cmd` if
/// `BCS_KMS_UNWRAP_CMD` is set.
pub fn resolve_unwrap_wrapper(provider: Option<&str>) -> Result<Arc<dyn KeyWrapper + Send + Sync>> {
    if let Some(provider) = provider {
        return match provider {
            "cmd" => Ok(Arc::new(CommandKeyWrapper::from_env_unwrap_only()?)),
            other => bcs_secrets::key_wrapper_for_provider(other)
                .map_err(anyhow::Error::msg)
                .with_context(|| format!("Failed to initialize KMS provider '{}'", other)),
        };
    }

    let mut multi = bcs_secrets::MultiKeyWrapper::from_available_env()
        .map_err(anyhow::Error::msg)
        .context("Failed to initialize native KMS providers")?;
    if std::env::var_os("BCS_KMS_UNWRAP_CMD").is_some() {
        multi.push(
            &["cmd"],
            Arc::new(CommandKeyWrapper::from_env_unwrap_only()?),
        );
    }
    if multi.is_empty() {
        anyhow::bail!(
            "No KMS unwrap backend available. Pass --kms-provider <name>, set BCS_KMS_UNWRAP_CMD, or configure credentials for: {}",
            bcs_secrets::available_kms_providers().join(", ")
        );
    }
    Ok(Arc::new(multi))
}

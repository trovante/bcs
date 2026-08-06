//! Kubernetes Secret resolver (kubectl-backed MVP).
//!
//! Feature: `kubernetes`
//!
//! Locators:
//! - `namespace/name/key`
//! - `name/key` (namespace from `BCS_K8S_NAMESPACE` or `default`)
//!
//! Uses `kubectl get secret -o jsonpath=...` so CI can mock kubectl.
//! Prefer workload identity for production; never log secret data.
//!
//! In-cluster HTTP mode can be added later; see docs/secrets.md.

use crate::cmd_runner::{default_runner, CommandRunner, SharedRunner};
use bcs_core::secret_resolver::SecretResolver;
use bcs_core::{BCSError, Result};
use std::sync::Arc;

/// Resolves `k8s:` / `kubernetes:` secret references via kubectl.
#[derive(Clone)]
pub struct KubernetesSecretResolver {
    default_namespace: String,
    runner: SharedRunner,
}

impl std::fmt::Debug for KubernetesSecretResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KubernetesSecretResolver")
            .field("default_namespace", &self.default_namespace)
            .finish()
    }
}

impl KubernetesSecretResolver {
    pub fn from_env() -> Result<Self> {
        let default_namespace = std::env::var("BCS_K8S_NAMESPACE")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "default".into());
        Ok(Self {
            default_namespace,
            runner: default_runner(),
        })
    }

    pub fn for_test(ns: impl Into<String>) -> Self {
        Self {
            default_namespace: ns.into(),
            runner: default_runner(),
        }
    }

    /// Inject a custom [`CommandRunner`] (tests / alternate backends).
    pub fn with_runner(ns: impl Into<String>, runner: Arc<dyn CommandRunner>) -> Self {
        Self {
            default_namespace: ns.into(),
            runner,
        }
    }

    fn parse_locator(&self, locator: &str) -> Result<(String, String, String)> {
        let parts: Vec<&str> = locator.split('/').collect();
        match parts.as_slice() {
            [name, key] => Ok((
                self.default_namespace.clone(),
                (*name).to_string(),
                (*key).to_string(),
            )),
            [ns, name, key] => Ok(((*ns).to_string(), (*name).to_string(), (*key).to_string())),
            _ => Err(BCSError::Decoding(
                "Kubernetes locator must be name/key or namespace/name/key".into(),
            )),
        }
    }
}

impl SecretResolver for KubernetesSecretResolver {
    fn resolve(&self, scheme: &str, locator: &str) -> Result<String> {
        if scheme != "k8s" && scheme != "kubernetes" {
            return Err(BCSError::Decoding(format!(
                "KubernetesSecretResolver does not handle scheme '{}'",
                scheme
            )));
        }
        let (ns, name, key) = self.parse_locator(locator)?;
        self.resolve_kubectl(&ns, &name, &key)
    }
}

impl KubernetesSecretResolver {
    fn resolve_kubectl(&self, ns: &str, name: &str, key: &str) -> Result<String> {
        let jsonpath = format!("jsonpath={{.data.{}}}", key);
        let output = self.runner.run(
            "kubectl",
            &["get", "secret", name, "-n", ns, "-o", &jsonpath],
        )?;
        if !output.success {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BCSError::Decoding(format!(
                "kubectl get secret failed: {}",
                stderr.trim()
            )));
        }
        let b64 = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if b64.is_empty() {
            return Err(BCSError::Decoding(format!(
                "Secret key '{}' empty or missing on {}/{}",
                key, ns, name
            )));
        }
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &b64)
            .map_err(|e| BCSError::Decoding(format!("Invalid base64 in secret data: {}", e)))?;
        String::from_utf8(bytes)
            .map_err(|e| BCSError::Decoding(format!("Secret data is not UTF-8: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd_runner::CommandOutput;
    use base64::Engine;

    struct FakeKubectl {
        b64: String,
        success: bool,
        stderr: String,
    }

    impl CommandRunner for FakeKubectl {
        fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput> {
            assert_eq!(program, "kubectl");
            assert_eq!(args.get(0).copied(), Some("get"));
            assert_eq!(args.get(1).copied(), Some("secret"));
            if self.success {
                Ok(CommandOutput::ok_stdout(self.b64.clone()))
            } else {
                Ok(CommandOutput::fail_stderr(self.stderr.clone()))
            }
        }
    }

    #[test]
    fn parse_two_and_three_segment_locators() {
        let r = KubernetesSecretResolver::for_test("apps");
        let (ns, name, key) = r.parse_locator("db/password").unwrap();
        assert_eq!(ns, "apps");
        assert_eq!(name, "db");
        assert_eq!(key, "password");
        let (ns, name, key) = r.parse_locator("prod/db/password").unwrap();
        assert_eq!(ns, "prod");
        assert_eq!(name, "db");
        assert_eq!(key, "password");
    }

    #[test]
    fn rejects_wrong_scheme() {
        let r = KubernetesSecretResolver::for_test("default");
        assert!(r.resolve("env", "x").is_err());
    }

    #[test]
    fn resolves_via_runner_and_decodes_base64() {
        let plain = "k8s-secret-value";
        let b64 = base64::engine::general_purpose::STANDARD.encode(plain);
        let r = KubernetesSecretResolver::with_runner(
            "apps",
            Arc::new(FakeKubectl {
                b64,
                success: true,
                stderr: String::new(),
            }),
        );
        assert_eq!(r.resolve("k8s", "db/password").unwrap(), plain);
    }

    #[test]
    fn surfaces_kubectl_failure() {
        let r = KubernetesSecretResolver::with_runner(
            "default",
            Arc::new(FakeKubectl {
                b64: String::new(),
                success: false,
                stderr: "NotFound".into(),
            }),
        );
        let err = r.resolve("kubernetes", "ns/name/key").unwrap_err().to_string();
        assert!(err.contains("NotFound"), "{err}");
    }

    #[test]
    fn rejects_bad_locator() {
        let r = KubernetesSecretResolver::for_test("default");
        assert!(r.resolve("k8s", "only-one").is_err());
    }
}

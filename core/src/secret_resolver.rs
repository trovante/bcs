//! Pluggable secret reference resolution.
//!
//! Phase 2 introduces [`SecretResolver`] and [`ResolverRegistry`]. Built-in
//! support is environment variables via [`EnvSecretResolver`]. Cloud/vault
//! providers register against schemes in later phases.

use crate::error::{BCSError, Result};
use std::collections::HashMap;
use std::sync::Arc;

/// Resolves a secret reference scheme + locator to a plaintext value.
///
/// Note: this trait is deliberately *not* `Send`/`Sync`. Remote providers and
/// the FFI callback resolver may hold non-thread-safe host state. Use
/// `Arc<dyn SecretResolver + Send + Sync>` in [`ResolverRegistry`] when sharing
/// across threads.
pub trait SecretResolver {
    fn resolve(&self, scheme: &str, locator: &str) -> Result<String>;
}

/// Built-in resolver that reads process environment variables.
///
/// Registered under the `env` scheme. The scheme argument is ignored at lookup
/// time; routing is done by [`ResolverRegistry`].
#[derive(Debug, Default, Clone, Copy)]
pub struct EnvSecretResolver;

impl SecretResolver for EnvSecretResolver {
    fn resolve(&self, _scheme: &str, locator: &str) -> Result<String> {
        std::env::var(locator).map_err(|_| {
            BCSError::Decoding(format!(
                "Failed to resolve secret reference 'env:{}': environment variable '{}' is not set",
                locator, locator
            ))
        })
    }
}

/// Maps URI schemes to resolvers and optionally remaps logical `secret:` refs.
#[derive(Clone, Default)]
pub struct ResolverRegistry {
    providers: HashMap<String, Arc<dyn SecretResolver + Send + Sync>>,
    /// When set, `__bcs_secret_ref__:secret:NAME` is routed to this scheme.
    default_scheme: Option<String>,
}

impl ResolverRegistry {
    /// Empty registry with no providers and no default scheme.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registry with [`EnvSecretResolver`] under `env`, and `secret:` remapped to `env`.
    pub fn with_env() -> Self {
        let mut registry = Self::new();
        registry.register("env", Arc::new(EnvSecretResolver));
        registry.set_default_scheme(Some("env".to_string()));
        registry
    }

    /// Register or replace the resolver for `scheme`.
    pub fn register(&mut self, scheme: &str, resolver: Arc<dyn SecretResolver + Send + Sync>) {
        self.providers.insert(scheme.to_string(), resolver);
    }

    /// Configure which scheme handles logical `secret:` references.
    ///
    /// Pass `None` so `secret:` refs fail until a default is configured.
    pub fn set_default_scheme(&mut self, scheme: Option<String>) {
        self.default_scheme = scheme;
    }

    /// Returns the configured default scheme for `secret:` refs, if any.
    pub fn default_scheme(&self) -> Option<&str> {
        self.default_scheme.as_deref()
    }

    /// Returns true when a provider is registered for `scheme`.
    pub fn has_scheme(&self, scheme: &str) -> bool {
        self.providers.contains_key(scheme)
    }

    fn resolve_scheme_and_locator<'a>(
        &'a self,
        scheme: &'a str,
        locator: &'a str,
    ) -> Result<(&'a str, &'a str)> {
        if scheme == "secret" {
            let default = self.default_scheme.as_deref().ok_or_else(|| {
                BCSError::Decoding(
                    "Failed to resolve secret reference 'secret:...': no default secret provider configured (use env: explicitly or set a default scheme)".to_string(),
                )
            })?;
            return Ok((default, locator));
        }
        Ok((scheme, locator))
    }
}

impl SecretResolver for ResolverRegistry {
    fn resolve(&self, scheme: &str, locator: &str) -> Result<String> {
        let (effective_scheme, effective_locator) =
            self.resolve_scheme_and_locator(scheme, locator)?;

        let provider = self.providers.get(effective_scheme).ok_or_else(|| {
            BCSError::Decoding(format!(
                "Failed to resolve secret reference '{}:{}': no provider registered for scheme '{}'",
                scheme, locator, effective_scheme
            ))
        })?;

        provider.resolve(effective_scheme, effective_locator)
    }
}

/// Build the CLI/default registry for a named provider.
///
/// Core always supports `env`. Remote backends live in the `bcs-secrets` crate.
pub fn registry_for_provider(provider: &str) -> Result<ResolverRegistry> {
    match provider {
        "env" => Ok(ResolverRegistry::with_env()),
        other => Err(BCSError::Decoding(format!(
            "Unknown secret provider '{}'. Core supports: env (enable bcs-secrets features for vault/aws)",
            other
        ))),
    }
}

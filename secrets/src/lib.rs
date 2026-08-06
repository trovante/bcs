//! Optional secret-provider backends for BCS.
//!
//! The core library keeps only the [`bcs_core::secret_resolver::SecretResolver`]
//! trait and the built-in `env` provider. This crate registers remote backends
//! behind Cargo features so the default CLI binary stays lean.
//!
//! Native KMS / Transit [`bcs_core::security::KeyWrapper`] implementations are
//! also feature-gated alongside the matching secret providers.

use bcs_core::secret_resolver::{
    registry_for_provider as core_registry_for_provider, ResolverRegistry,
};
use bcs_core::security::KeyWrapper;
use bcs_core::{BCSError, Result};
use std::sync::Arc;

#[cfg(feature = "vault")]
pub mod vault;

#[cfg(feature = "aws")]
pub mod aws;

#[cfg(feature = "azure")]
pub mod azure;

#[cfg(feature = "gcp")]
pub mod gcp;

#[cfg(feature = "doppler")]
pub mod doppler;

#[cfg(feature = "infisical")]
pub mod infisical;

#[cfg(feature = "akeyless")]
pub mod akeyless;

#[cfg(feature = "bitwarden")]
pub mod bitwarden;

#[cfg(feature = "onepassword")]
pub mod onepassword;

#[cfg(feature = "kubernetes")]
pub mod kubernetes;

/// Injectable CLI runner for `op` / `kubectl` backends (tests + production).
#[cfg(any(feature = "onepassword", feature = "kubernetes"))]
pub mod cmd_runner;

#[cfg(feature = "aws")]
pub mod kms_aws;

#[cfg(feature = "azure")]
pub mod kms_azure;

#[cfg(feature = "gcp")]
pub mod kms_gcp;

#[cfg(feature = "vault")]
pub mod kms_vault;

#[cfg(any(
    feature = "azure",
    feature = "gcp",
    feature = "doppler",
    feature = "infisical",
    feature = "akeyless",
    feature = "bitwarden"
))]
mod http_util;

/// Shared locator helpers used by providers.
pub mod locator;

#[cfg(any(
    feature = "vault",
    feature = "aws",
    feature = "azure",
    feature = "gcp",
    feature = "doppler",
    feature = "infisical",
    feature = "akeyless",
    feature = "bitwarden"
))]
macro_rules! register_provider {
    ($registry:ident, $scheme:expr, $resolver:expr) => {{
        $registry.register($scheme, Arc::new($resolver));
        $registry.set_default_scheme(Some($scheme.to_string()));
        Ok($registry)
    }};
}

/// Build a [`ResolverRegistry`] for a named provider.
///
/// Always supports `env`. Feature-gated backends are listed by [`available_providers`].
/// The selected provider becomes the default scheme for logical `secret:` refs.
/// The `env` scheme remains registered so mixed files keep working.
pub fn registry_for_provider(provider: &str) -> Result<ResolverRegistry> {
    match provider {
        "env" => core_registry_for_provider("env"),
        #[cfg(feature = "vault")]
        "vault" => {
            let mut registry = ResolverRegistry::with_env();
            register_provider!(registry, "vault", vault::VaultSecretResolver::from_env()?)
        }
        #[cfg(any(feature = "vault", feature = "openbao"))]
        "openbao" => {
            let mut registry = ResolverRegistry::with_env();
            register_provider!(
                registry,
                "openbao",
                vault::VaultSecretResolver::from_openbao_env()?
            )
        }
        #[cfg(feature = "aws")]
        "aws" => {
            let mut registry = ResolverRegistry::with_env();
            register_provider!(registry, "aws", aws::AwsSecretResolver::from_env()?)
        }
        #[cfg(feature = "azure")]
        "azure" => {
            let mut registry = ResolverRegistry::with_env();
            register_provider!(registry, "azure", azure::AzureSecretResolver::from_env()?)
        }
        #[cfg(feature = "gcp")]
        "gcp" => {
            let mut registry = ResolverRegistry::with_env();
            register_provider!(registry, "gcp", gcp::GcpSecretResolver::from_env()?)
        }
        #[cfg(feature = "doppler")]
        "doppler" => {
            let mut registry = ResolverRegistry::with_env();
            register_provider!(
                registry,
                "doppler",
                doppler::DopplerSecretResolver::from_env()?
            )
        }
        #[cfg(feature = "infisical")]
        "infisical" => {
            let mut registry = ResolverRegistry::with_env();
            register_provider!(
                registry,
                "infisical",
                infisical::InfisicalSecretResolver::from_env()?
            )
        }
        #[cfg(feature = "akeyless")]
        "akeyless" => {
            let mut registry = ResolverRegistry::with_env();
            register_provider!(
                registry,
                "akeyless",
                akeyless::AkeylessSecretResolver::from_env()?
            )
        }
        #[cfg(feature = "bitwarden")]
        "bitwarden" => {
            let mut registry = ResolverRegistry::with_env();
            register_provider!(
                registry,
                "bitwarden",
                bitwarden::BitwardenSecretResolver::from_env()?
            )
        }
        #[cfg(feature = "onepassword")]
        "op" | "onepassword" => {
            let mut registry = ResolverRegistry::with_env();
            let resolver = Arc::new(onepassword::OnePasswordSecretResolver::from_env()?);
            registry.register("op", resolver.clone());
            registry.register("onepassword", resolver);
            registry.set_default_scheme(Some("op".to_string()));
            Ok(registry)
        }
        #[cfg(feature = "kubernetes")]
        "k8s" | "kubernetes" => {
            let mut registry = ResolverRegistry::with_env();
            let resolver = Arc::new(kubernetes::KubernetesSecretResolver::from_env()?);
            registry.register("k8s", resolver.clone());
            registry.register("kubernetes", resolver);
            registry.set_default_scheme(Some("k8s".to_string()));
            Ok(registry)
        }
        other => Err(BCSError::Decoding(format!(
            "Unknown or disabled secret provider '{}'. Available: {}",
            other,
            available_providers().join(", ")
        ))),
    }
}

/// Provider names compiled into this build.
pub fn available_providers() -> Vec<&'static str> {
    // `mut` only used when optional provider features are enabled.
    #[allow(unused_mut)]
    let mut names = vec!["env"];
    #[cfg(feature = "vault")]
    names.push("vault");
    #[cfg(any(feature = "vault", feature = "openbao"))]
    names.push("openbao");
    #[cfg(feature = "aws")]
    names.push("aws");
    #[cfg(feature = "azure")]
    names.push("azure");
    #[cfg(feature = "gcp")]
    names.push("gcp");
    #[cfg(feature = "doppler")]
    names.push("doppler");
    #[cfg(feature = "infisical")]
    names.push("infisical");
    #[cfg(feature = "akeyless")]
    names.push("akeyless");
    #[cfg(feature = "bitwarden")]
    names.push("bitwarden");
    #[cfg(feature = "onepassword")]
    {
        names.push("op");
        names.push("onepassword");
    }
    #[cfg(feature = "kubernetes")]
    {
        names.push("k8s");
        names.push("kubernetes");
    }
    names
}

/// Provider names that have a native [`KeyWrapper`] in this build.
pub fn available_kms_providers() -> Vec<&'static str> {
    // `mut` only used when optional KMS features are enabled.
    #[allow(unused_mut)]
    let mut names = Vec::new();
    #[cfg(feature = "aws")]
    names.push("aws");
    #[cfg(feature = "azure")]
    names.push("azure");
    #[cfg(feature = "gcp")]
    names.push("gcp");
    #[cfg(feature = "vault")]
    {
        names.push("vault");
        names.push("openbao");
    }
    names
}

/// Build a native [`KeyWrapper`] for the given KMS/Transit provider name.
pub fn key_wrapper_for_provider(provider: &str) -> Result<Arc<dyn KeyWrapper + Send + Sync>> {
    match provider {
        #[cfg(feature = "aws")]
        "aws" | "aws-kms" => Ok(Arc::new(kms_aws::AwsKmsKeyWrapper::from_env()?)),
        #[cfg(feature = "azure")]
        "azure" | "azure-kms" | "akv" => Ok(Arc::new(kms_azure::AzureKmsKeyWrapper::from_env()?)),
        #[cfg(feature = "gcp")]
        "gcp" | "gcp-kms" | "google" => Ok(Arc::new(kms_gcp::GcpKmsKeyWrapper::from_env()?)),
        #[cfg(feature = "vault")]
        "vault" | "vault-transit" | "transit" => {
            Ok(Arc::new(kms_vault::VaultTransitKeyWrapper::from_env()?))
        }
        #[cfg(any(feature = "vault", feature = "openbao"))]
        "openbao" | "bao" => Ok(Arc::new(
            kms_vault::VaultTransitKeyWrapper::from_openbao_env()?,
        )),
        other => Err(BCSError::Decoding(format!(
            "Unknown or disabled KMS provider '{}'. Available: {}",
            other,
            available_kms_providers().join(", ")
        ))),
    }
}

/// Composite wrapper that dispatches by the `provider` field stored in kms markers.
pub struct MultiKeyWrapper {
    wrappers: Vec<(Vec<&'static str>, Arc<dyn KeyWrapper + Send + Sync>)>,
}

impl MultiKeyWrapper {
    pub fn new() -> Self {
        Self {
            wrappers: Vec::new(),
        }
    }

    pub fn push(&mut self, aliases: &[&'static str], wrapper: Arc<dyn KeyWrapper + Send + Sync>) {
        self.wrappers.push((aliases.to_vec(), wrapper));
    }

    pub fn is_empty(&self) -> bool {
        self.wrappers.is_empty()
    }

    pub fn from_available_env() -> Result<Self> {
        let mut multi = Self::new();
        for name in available_kms_providers() {
            // Avoid double-registering openbao when vault feature also lists it.
            if name == "openbao" {
                #[cfg(feature = "vault")]
                {
                    if let Ok(w) = kms_vault::VaultTransitKeyWrapper::from_openbao_env() {
                        multi.push(&["openbao", "bao"], Arc::new(w));
                    }
                }
                continue;
            }
            if let Ok(w) = key_wrapper_for_provider(name) {
                let aliases: &[&'static str] = match name {
                    "aws" => &["aws", "aws-kms"],
                    "azure" => &["azure", "azure-kms", "akv"],
                    "gcp" => &["gcp", "gcp-kms", "google"],
                    "vault" => &["vault", "vault-transit", "transit"],
                    _ => &[name],
                };
                multi.push(aliases, w);
            }
        }
        Ok(multi)
    }

    fn find(&self, provider: &str) -> Result<&dyn KeyWrapper> {
        for (aliases, wrapper) in &self.wrappers {
            if aliases.contains(&provider) {
                return Ok(wrapper.as_ref());
            }
        }
        Err(BCSError::Decoding(format!(
            "No KMS wrapper registered for provider '{}' (available: {})",
            provider,
            available_kms_providers().join(", ")
        )))
    }
}

impl Default for MultiKeyWrapper {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyWrapper for MultiKeyWrapper {
    fn wrap(&self, provider: &str, kek_locator: &str, dek: &[u8]) -> Result<Vec<u8>> {
        self.find(provider)?.wrap(provider, kek_locator, dek)
    }

    fn unwrap(&self, provider: &str, kek_locator: &str, wrapped_dek: &[u8]) -> Result<[u8; 32]> {
        self.find(provider)?
            .unwrap(provider, kek_locator, wrapped_dek)
    }
}

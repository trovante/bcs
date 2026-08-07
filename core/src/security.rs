use crate::error::{BCSError, Result};
use crate::index::{parse_path, PathSegment};
use crate::limits;
use crate::types::Value;
use aes_gcm::aead::{Aead, AeadCore, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;

type Aes256Nonce = Nonce<<Aes256Gcm as AeadCore>::NonceSize>;

/// Marker prefix for password-protected (`pbkdf2`) field values.
pub const PREFIX_PBKDF2: &str = "__bcs_sensitive_pbkdf2__:";
/// Marker prefix for KMS-wrapped (`kms`) field values.
pub const PREFIX_KMS: &str = "__bcs_sensitive_kms__:";

/// Marker prefix for secret references (resolved from the environment at decode time).
const SECRET_REF_PREFIX: &str = "__bcs_secret_ref__:";

/// Placeholder used when secret refs are masked without resolving.
const SECRET_REF_MASK: &str = "[SECRET_REF]";

/// Logical scheme name for password-based protect (CLI / docs).
pub const SCHEME_PBKDF2: &str = "pbkdf2";
/// Logical scheme name for KMS-wrapped DEK protect (CLI / docs).
pub const SCHEME_KMS: &str = "kms";

const PBKDF2_ITERATIONS: u32 = 120_000;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const DEK_LEN: usize = 32;

/// Parsed secret reference scheme + locator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretRef {
    /// URI scheme (`env`, `secret`, or a future provider scheme).
    pub scheme: String,
    /// Locator passed to the provider (env var name, vault path, etc.).
    pub name: String,
}

/// Host-provided DEK wrap/unwrap (cloud KMS, Vault Transit, external command, etc.).
///
/// Not required to be `Send`/`Sync`; callers that need sharing use `Arc<dyn KeyWrapper + Send + Sync>`.
pub trait KeyWrapper {
    fn wrap(&self, provider: &str, kek_locator: &str, dek: &[u8]) -> Result<Vec<u8>>;
    fn unwrap(
        &self,
        provider: &str,
        kek_locator: &str,
        wrapped_dek: &[u8],
    ) -> Result<[u8; DEK_LEN]>;
}

pub use crate::secret_resolver::{
    registry_for_provider, EnvSecretResolver, ResolverRegistry, SecretResolver,
};

enum ProtectMode<'a> {
    Password(&'a str),
    Kms {
        provider: &'a str,
        kek_locator: &'a str,
        wrapper: &'a dyn KeyWrapper,
    },
}

/// Protect sensitive fields in a value tree using password-based encryption (`pbkdf2` scheme).
pub fn protect_paths(value: &mut Value, paths: &[String], password: &str) -> Result<()> {
    if password.is_empty() {
        return Err(BCSError::Encoding(
            "Password for sensitive field protection cannot be empty".to_string(),
        ));
    }

    protect_paths_with(value, paths, ProtectMode::Password(password))
}

/// Protect sensitive fields using a host `KeyWrapper` (`kms` scheme).
pub fn protect_paths_kms(
    value: &mut Value,
    paths: &[String],
    provider: &str,
    kek_locator: &str,
    wrapper: &dyn KeyWrapper,
) -> Result<()> {
    if provider.is_empty() {
        return Err(BCSError::Encoding(
            "KMS provider must be non-empty".to_string(),
        ));
    }
    if kek_locator.is_empty() {
        return Err(BCSError::Encoding(
            "KMS key locator must be non-empty".to_string(),
        ));
    }
    if provider.len() > 255 {
        return Err(BCSError::Encoding(
            "KMS provider name is too long (max 255 bytes)".to_string(),
        ));
    }
    if kek_locator.len() > u16::MAX as usize {
        return Err(BCSError::Encoding(
            "KMS key locator is too long".to_string(),
        ));
    }

    protect_paths_with(
        value,
        paths,
        ProtectMode::Kms {
            provider,
            kek_locator,
            wrapper,
        },
    )
}

fn protect_paths_with(value: &mut Value, paths: &[String], mode: ProtectMode<'_>) -> Result<()> {
    for path in paths {
        let segments = parse_path(path)?;
        if segments.is_empty() {
            return Err(BCSError::Encoding(format!(
                "Sensitive path '{}' is empty",
                path
            )));
        }

        protect_at_segments(value, &segments, &mode, path)?;
    }

    Ok(())
}

/// Reveal sensitive fields at paths using password-based decryption (`pbkdf2` only).
pub fn reveal_paths(value: &mut Value, paths: &[String], password: &str) -> Result<()> {
    if password.is_empty() {
        return Err(BCSError::Decoding(
            "Password for sensitive field reveal cannot be empty".to_string(),
        ));
    }

    for path in paths {
        let segments = parse_path(path)?;
        if segments.is_empty() {
            return Err(BCSError::Decoding(format!(
                "Sensitive path '{}' is empty",
                path
            )));
        }

        reveal_at_segments(value, &segments, Some(password), None, path)?;
    }

    Ok(())
}

/// Reveal all protected markers recursively using a password (`pbkdf2` markers only).
pub fn reveal_all(value: &mut Value, password: &str) -> Result<()> {
    if password.is_empty() {
        return Err(BCSError::Decoding(
            "Password for sensitive field reveal cannot be empty".to_string(),
        ));
    }

    reveal_all_ex(value, Some(password), None)
}

/// Reveal all protected markers, dispatching by scheme tag (`pbkdf2` / `kms`).
pub fn reveal_all_ex(
    value: &mut Value,
    password: Option<&str>,
    wrapper: Option<&dyn KeyWrapper>,
) -> Result<()> {
    if let Some(pass) = password {
        if pass.is_empty() {
            return Err(BCSError::Decoding(
                "Password for sensitive field reveal cannot be empty".to_string(),
            ));
        }
    }

    reveal_all_recursive(value, password, wrapper, 0)
}

/// Mask all protected markers recursively without decrypting.
pub fn mask_all(value: &mut Value) -> Result<()> {
    mask_all_recursive(value, 0)
}

/// Mask all secret-reference markers recursively without resolving.
pub fn mask_secret_refs(value: &mut Value) -> Result<()> {
    mask_secret_refs_recursive(value, 0)
}

/// Mask both password-protected markers and secret references.
pub fn mask_sensitive_fields(value: &mut Value) -> Result<()> {
    mask_all(value)?;
    mask_secret_refs(value)
}

/// Return true when value contains an encrypted sensitive marker string.
pub fn is_protected_marker(value: &Value) -> bool {
    matches!(
        value,
        Value::String(s) if s.starts_with(PREFIX_PBKDF2) || s.starts_with(PREFIX_KMS)
    )
}

/// Return true when value is a secret-reference marker string.
pub fn is_secret_ref_marker(value: &Value) -> bool {
    matches!(value, Value::String(s) if s.starts_with(SECRET_REF_PREFIX))
}

/// Parse a secret-reference marker into scheme and locator.
///
/// Expected form: `__bcs_secret_ref__:<scheme>:<locator>`.
/// Scheme must match `[a-z][a-z0-9+.-]*`. Provider availability is checked at resolve time.
pub fn parse_secret_ref(marker: &str) -> Result<SecretRef> {
    let Some(uri) = marker.strip_prefix(SECRET_REF_PREFIX) else {
        return Err(BCSError::Decoding(
            "Value is not a secret reference marker".to_string(),
        ));
    };

    let (scheme, name) = uri.split_once(':').ok_or_else(|| {
        BCSError::Decoding(format!(
            "Invalid secret reference URI '{}': expected scheme:locator",
            uri
        ))
    })?;

    if !is_valid_scheme(scheme) {
        return Err(BCSError::Decoding(format!(
            "Invalid secret reference scheme '{}': expected [a-z][a-z0-9+.-]*",
            scheme
        )));
    }

    if name.is_empty() {
        return Err(BCSError::Decoding(format!(
            "Invalid secret reference URI '{}': locator must be non-empty",
            uri
        )));
    }

    Ok(SecretRef {
        scheme: scheme.to_string(),
        name: name.to_string(),
    })
}

/// Resolve all secret-reference markers using the provided resolver.
///
/// Use [`ResolverRegistry::with_env`] for environment-backed resolution.
/// Logical `secret:` refs are remapped by the registry default scheme when configured.
pub fn resolve_secret_refs(value: &mut Value, resolver: &dyn SecretResolver) -> Result<()> {
    resolve_secret_refs_recursive(value, resolver, 0)
}

/// Build a secret-reference marker string for the given scheme and locator.
pub fn format_secret_ref(scheme: &str, locator: &str) -> Result<String> {
    if !is_valid_scheme(scheme) {
        return Err(BCSError::Encoding(format!(
            "Invalid secret reference scheme '{}': expected [a-z][a-z0-9+.-]*",
            scheme
        )));
    }
    if locator.is_empty() {
        return Err(BCSError::Encoding(
            "Secret reference locator must be non-empty".to_string(),
        ));
    }
    Ok(format!("{}{}:{}", SECRET_REF_PREFIX, scheme, locator))
}

fn is_valid_scheme(scheme: &str) -> bool {
    let mut chars = scheme.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '+' | '.' | '-'))
}

fn protect_at_segments(
    value: &mut Value,
    segments: &[PathSegment],
    mode: &ProtectMode<'_>,
    path: &str,
) -> Result<()> {
    protect_at_segments_at(value, segments, mode, path, 0)
}

fn protect_at_segments_at(
    value: &mut Value,
    segments: &[PathSegment],
    mode: &ProtectMode<'_>,
    path: &str,
    depth: usize,
) -> Result<()> {
    limits::ensure_depth(depth)?;
    if segments.is_empty() {
        let marker = match mode {
            ProtectMode::Password(password) => encrypt_value_pbkdf2(value, password)?,
            ProtectMode::Kms {
                provider,
                kek_locator,
                wrapper,
            } => encrypt_value_kms(value, provider, kek_locator, *wrapper)?,
        };
        *value = Value::String(marker);
        return Ok(());
    }

    match &segments[0] {
        PathSegment::Field(field_name) => match value {
            Value::Struct(fields) => {
                for (name, _hash, child) in fields.iter_mut() {
                    if name == field_name {
                        return protect_at_segments_at(
                            child,
                            &segments[1..],
                            mode,
                            path,
                            depth + 1,
                        );
                    }
                }
                Err(BCSError::Encoding(format!(
                    "Sensitive path '{}' not found (missing field '{}')",
                    path, field_name
                )))
            }
            Value::Map(entries) => {
                for (key, child) in entries.iter_mut() {
                    if let Value::String(key_str) = key {
                        if key_str == field_name {
                            return protect_at_segments_at(
                                child,
                                &segments[1..],
                                mode,
                                path,
                                depth + 1,
                            );
                        }
                    }
                }
                Err(BCSError::Encoding(format!(
                    "Sensitive path '{}' not found (missing map key '{}')",
                    path, field_name
                )))
            }
            _ => Err(BCSError::Encoding(format!(
                "Sensitive path '{}' cannot traverse non-object at '{}'",
                path, field_name
            ))),
        },
        PathSegment::Index(index) => match value {
            Value::List(items) => {
                if *index >= items.len() {
                    return Err(BCSError::Encoding(format!(
                        "Sensitive path '{}' index {} out of bounds",
                        path, index
                    )));
                }
                protect_at_segments_at(&mut items[*index], &segments[1..], mode, path, depth + 1)
            }
            _ => Err(BCSError::Encoding(format!(
                "Sensitive path '{}' cannot index non-list value",
                path
            ))),
        },
        PathSegment::WildcardIndex => Err(BCSError::Encoding(format!(
            "Sensitive path '{}' cannot use wildcards",
            path
        ))),
    }
}

fn reveal_at_segments(
    value: &mut Value,
    segments: &[PathSegment],
    password: Option<&str>,
    wrapper: Option<&dyn KeyWrapper>,
    _path: &str,
) -> Result<()> {
    reveal_at_segments_at(value, segments, password, wrapper, 0)
}

fn reveal_at_segments_at(
    value: &mut Value,
    segments: &[PathSegment],
    password: Option<&str>,
    wrapper: Option<&dyn KeyWrapper>,
    depth: usize,
) -> Result<()> {
    limits::ensure_depth(depth)?;
    if segments.is_empty() {
        let decrypted = decrypt_marker_to_value(value, password, wrapper)?;
        *value = decrypted;
        return Ok(());
    }

    match &segments[0] {
        PathSegment::Field(field_name) => match value {
            Value::Struct(fields) => {
                for (name, _hash, child) in fields.iter_mut() {
                    if name == field_name {
                        return reveal_at_segments_at(
                            child,
                            &segments[1..],
                            password,
                            wrapper,
                            depth + 1,
                        );
                    }
                }
                Ok(())
            }
            Value::Map(entries) => {
                for (key, child) in entries.iter_mut() {
                    if let Value::String(key_str) = key {
                        if key_str == field_name {
                            return reveal_at_segments_at(
                                child,
                                &segments[1..],
                                password,
                                wrapper,
                                depth + 1,
                            );
                        }
                    }
                }
                Ok(())
            }
            _ => Ok(()),
        },
        PathSegment::Index(index) => match value {
            Value::List(items) => {
                if *index >= items.len() {
                    return Ok(());
                }
                reveal_at_segments_at(
                    &mut items[*index],
                    &segments[1..],
                    password,
                    wrapper,
                    depth + 1,
                )
            }
            _ => Ok(()),
        },
        PathSegment::WildcardIndex => Ok(()),
    }
}

fn marker_from_payload(prefix: &str, payload: &[u8]) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(payload);
    format!("{}{}", prefix, encoded)
}

fn encrypt_value_pbkdf2(value: &Value, password: &str) -> Result<String> {
    let plaintext = serde_json::to_vec(value)
        .map_err(|e| BCSError::Encoding(format!("Failed to serialize sensitive value: {}", e)))?;

    let mut salt = [0u8; SALT_LEN];
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::fill(&mut salt);
    rand::fill(&mut nonce_bytes);

    let key = derive_key(password, &salt, PBKDF2_ITERATIONS);
    let ciphertext = aes_encrypt(&key, &nonce_bytes, &plaintext)?;

    let mut body = Vec::with_capacity(4 + SALT_LEN + NONCE_LEN + ciphertext.len());
    body.extend_from_slice(&PBKDF2_ITERATIONS.to_le_bytes());
    body.extend_from_slice(&salt);
    body.extend_from_slice(&nonce_bytes);
    body.extend_from_slice(&ciphertext);

    Ok(marker_from_payload(PREFIX_PBKDF2, &body))
}

fn encrypt_value_kms(
    value: &Value,
    provider: &str,
    kek_locator: &str,
    wrapper: &dyn KeyWrapper,
) -> Result<String> {
    let plaintext = serde_json::to_vec(value)
        .map_err(|e| BCSError::Encoding(format!("Failed to serialize sensitive value: {}", e)))?;

    let mut dek = [0u8; DEK_LEN];
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::fill(&mut dek);
    rand::fill(&mut nonce_bytes);

    let ciphertext = aes_encrypt(&dek, &nonce_bytes, &plaintext)?;
    let wrapped_dek = wrapper.wrap(provider, kek_locator, &dek)?;
    if wrapped_dek.len() > u16::MAX as usize {
        return Err(BCSError::Encoding("Wrapped DEK is too long".to_string()));
    }

    let provider_bytes = provider.as_bytes();
    let kek_bytes = kek_locator.as_bytes();
    let mut body = Vec::with_capacity(
        1 + provider_bytes.len()
            + 2
            + kek_bytes.len()
            + 2
            + wrapped_dek.len()
            + NONCE_LEN
            + ciphertext.len(),
    );
    body.push(provider_bytes.len() as u8);
    body.extend_from_slice(provider_bytes);
    body.extend_from_slice(&(kek_bytes.len() as u16).to_le_bytes());
    body.extend_from_slice(kek_bytes);
    body.extend_from_slice(&(wrapped_dek.len() as u16).to_le_bytes());
    body.extend_from_slice(&wrapped_dek);
    body.extend_from_slice(&nonce_bytes);
    body.extend_from_slice(&ciphertext);

    Ok(marker_from_payload(PREFIX_KMS, &body))
}

fn aes_nonce(nonce_bytes: &[u8]) -> Result<Aes256Nonce> {
    Aes256Nonce::try_from(nonce_bytes).map_err(|_| {
        BCSError::Encoding(format!(
            "Invalid AES-GCM nonce length: expected {}, got {}",
            NONCE_LEN,
            nonce_bytes.len()
        ))
    })
}

fn aes_encrypt(key: &[u8], nonce_bytes: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| BCSError::Encoding(format!("Failed to initialize cipher: {}", e)))?;
    let nonce = aes_nonce(nonce_bytes)?;
    cipher
        .encrypt(&nonce, plaintext)
        .map_err(|_| BCSError::Encoding("Failed to encrypt sensitive value".to_string()))
}

fn aes_decrypt(key: &[u8], nonce_bytes: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| BCSError::Decoding(format!("Failed to initialize cipher: {}", e)))?;
    let nonce = aes_nonce(nonce_bytes).map_err(|e| BCSError::Decoding(e.to_string()))?;
    cipher.decrypt(&nonce, ciphertext).map_err(|_| {
        BCSError::Decoding("Failed to decrypt sensitive value (wrong password or key?)".to_string())
    })
}

fn decrypt_marker_to_value(
    value: &Value,
    password: Option<&str>,
    wrapper: Option<&dyn KeyWrapper>,
) -> Result<Value> {
    let marker = match value {
        Value::String(s) => s,
        _ => return Ok(value.clone()),
    };

    if marker.starts_with("__bcs_sensitive__:") {
        return Err(BCSError::Decoding(
            "Obsolete sensitive marker prefix '__bcs_sensitive__:'; use '__bcs_sensitive_pbkdf2__:' or '__bcs_sensitive_kms__:'"
                .to_string(),
        ));
    }

    if let Some(payload_b64) = marker.strip_prefix(PREFIX_PBKDF2) {
        let pass = password.ok_or_else(|| {
            BCSError::Decoding("Password required to reveal pbkdf2-protected fields".to_string())
        })?;
        let payload = base64::engine::general_purpose::STANDARD
            .decode(payload_b64)
            .map_err(|_| BCSError::Decoding("Invalid protected payload encoding".to_string()))?;
        return decrypt_pbkdf2_body(&payload, pass);
    }

    if let Some(payload_b64) = marker.strip_prefix(PREFIX_KMS) {
        let wrapper = wrapper.ok_or_else(|| {
            BCSError::Decoding("Key wrapper required to reveal kms-protected fields".to_string())
        })?;
        let payload = base64::engine::general_purpose::STANDARD
            .decode(payload_b64)
            .map_err(|_| BCSError::Decoding("Invalid protected payload encoding".to_string()))?;
        return decrypt_kms_body(&payload, wrapper);
    }

    Ok(value.clone())
}

fn decrypt_pbkdf2_body(body: &[u8], password: &str) -> Result<Value> {
    if body.len() <= 4 + SALT_LEN + NONCE_LEN {
        return Err(BCSError::Decoding(
            "Invalid protected payload length".to_string(),
        ));
    }

    let iterations = u32::from_le_bytes([body[0], body[1], body[2], body[3]]);
    if iterations == 0 {
        return Err(BCSError::Decoding(
            "Invalid protected payload PBKDF2 iterations".to_string(),
        ));
    }

    let rest = &body[4..];
    let salt = &rest[0..SALT_LEN];
    let nonce_bytes = &rest[SALT_LEN..SALT_LEN + NONCE_LEN];
    let ciphertext = &rest[SALT_LEN + NONCE_LEN..];

    let key = derive_key(password, salt, iterations);
    let plaintext = aes_decrypt(&key, nonce_bytes, ciphertext)?;
    serde_json::from_slice::<Value>(&plaintext)
        .map_err(|e| BCSError::Decoding(format!("Failed to decode sensitive value: {}", e)))
}

fn decrypt_kms_body(body: &[u8], wrapper: &dyn KeyWrapper) -> Result<Value> {
    if body.is_empty() {
        return Err(BCSError::Decoding(
            "Invalid kms protected payload length".to_string(),
        ));
    }

    let mut offset = 0usize;
    let provider_len = body[offset] as usize;
    offset += 1;
    if body.len() < offset + provider_len + 2 {
        return Err(BCSError::Decoding(
            "Invalid kms protected payload length".to_string(),
        ));
    }
    let provider = std::str::from_utf8(&body[offset..offset + provider_len])
        .map_err(|_| BCSError::Decoding("Invalid kms provider encoding".to_string()))?;
    offset += provider_len;

    let kek_len = u16::from_le_bytes([body[offset], body[offset + 1]]) as usize;
    offset += 2;
    if body.len() < offset + kek_len + 2 {
        return Err(BCSError::Decoding(
            "Invalid kms protected payload length".to_string(),
        ));
    }
    let kek_locator = std::str::from_utf8(&body[offset..offset + kek_len])
        .map_err(|_| BCSError::Decoding("Invalid kms key locator encoding".to_string()))?;
    offset += kek_len;

    let wrapped_len = u16::from_le_bytes([body[offset], body[offset + 1]]) as usize;
    offset += 2;
    if body.len() < offset + wrapped_len + NONCE_LEN {
        return Err(BCSError::Decoding(
            "Invalid kms protected payload length".to_string(),
        ));
    }
    let wrapped_dek = &body[offset..offset + wrapped_len];
    offset += wrapped_len;
    let nonce_bytes = &body[offset..offset + NONCE_LEN];
    let ciphertext = &body[offset + NONCE_LEN..];

    let dek = wrapper.unwrap(provider, kek_locator, wrapped_dek)?;
    let plaintext = aes_decrypt(&dek, nonce_bytes, ciphertext)?;
    serde_json::from_slice::<Value>(&plaintext)
        .map_err(|e| BCSError::Decoding(format!("Failed to decode sensitive value: {}", e)))
}

fn derive_key(password: &str, salt: &[u8], iterations: u32) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, iterations, &mut key);
    key
}

fn reveal_all_recursive(
    value: &mut Value,
    password: Option<&str>,
    wrapper: Option<&dyn KeyWrapper>,
    depth: usize,
) -> Result<()> {
    limits::ensure_depth(depth)?;
    match value {
        Value::String(_) => {
            let decrypted = decrypt_marker_to_value(value, password, wrapper)?;
            *value = decrypted;
            Ok(())
        }
        Value::List(items) => {
            for item in items.iter_mut() {
                reveal_all_recursive(item, password, wrapper, depth + 1)?;
            }
            Ok(())
        }
        Value::Map(entries) => {
            for (_key, val) in entries.iter_mut() {
                reveal_all_recursive(val, password, wrapper, depth + 1)?;
            }
            Ok(())
        }
        Value::Struct(fields) => {
            for (_name, _hash, val) in fields.iter_mut() {
                reveal_all_recursive(val, password, wrapper, depth + 1)?;
            }
            Ok(())
        }
        Value::Optional(Some(inner)) => reveal_all_recursive(inner, password, wrapper, depth + 1),
        Value::Union(_, inner) => reveal_all_recursive(inner, password, wrapper, depth + 1),
        _ => Ok(()),
    }
}

fn mask_all_recursive(value: &mut Value, depth: usize) -> Result<()> {
    limits::ensure_depth(depth)?;
    match value {
        Value::String(s) if s.starts_with(PREFIX_PBKDF2) || s.starts_with(PREFIX_KMS) => {
            *value = Value::String("[PROTECTED]".to_string());
            Ok(())
        }
        Value::List(items) => {
            for item in items.iter_mut() {
                mask_all_recursive(item, depth + 1)?;
            }
            Ok(())
        }
        Value::Map(entries) => {
            for (_key, val) in entries.iter_mut() {
                mask_all_recursive(val, depth + 1)?;
            }
            Ok(())
        }
        Value::Struct(fields) => {
            for (_name, _hash, val) in fields.iter_mut() {
                mask_all_recursive(val, depth + 1)?;
            }
            Ok(())
        }
        Value::Optional(Some(inner)) => mask_all_recursive(inner, depth + 1),
        Value::Union(_, inner) => mask_all_recursive(inner, depth + 1),
        _ => Ok(()),
    }
}

fn mask_secret_refs_recursive(value: &mut Value, depth: usize) -> Result<()> {
    limits::ensure_depth(depth)?;
    match value {
        Value::String(s) if s.starts_with(SECRET_REF_PREFIX) => {
            *value = Value::String(SECRET_REF_MASK.to_string());
            Ok(())
        }
        Value::List(items) => {
            for item in items.iter_mut() {
                mask_secret_refs_recursive(item, depth + 1)?;
            }
            Ok(())
        }
        Value::Map(entries) => {
            for (_key, val) in entries.iter_mut() {
                mask_secret_refs_recursive(val, depth + 1)?;
            }
            Ok(())
        }
        Value::Struct(fields) => {
            for (_name, _hash, val) in fields.iter_mut() {
                mask_secret_refs_recursive(val, depth + 1)?;
            }
            Ok(())
        }
        Value::Optional(Some(inner)) => mask_secret_refs_recursive(inner, depth + 1),
        Value::Union(_, inner) => mask_secret_refs_recursive(inner, depth + 1),
        _ => Ok(()),
    }
}

fn resolve_secret_refs_recursive(
    value: &mut Value,
    resolver: &dyn SecretResolver,
    depth: usize,
) -> Result<()> {
    limits::ensure_depth(depth)?;
    match value {
        Value::String(s) if s.starts_with(SECRET_REF_PREFIX) => {
            let resolved = resolve_secret_ref_marker(s, resolver)?;
            *value = Value::String(resolved);
            Ok(())
        }
        Value::List(items) => {
            for item in items.iter_mut() {
                resolve_secret_refs_recursive(item, resolver, depth + 1)?;
            }
            Ok(())
        }
        Value::Map(entries) => {
            for (_key, val) in entries.iter_mut() {
                resolve_secret_refs_recursive(val, resolver, depth + 1)?;
            }
            Ok(())
        }
        Value::Struct(fields) => {
            for (_name, _hash, val) in fields.iter_mut() {
                resolve_secret_refs_recursive(val, resolver, depth + 1)?;
            }
            Ok(())
        }
        Value::Optional(Some(inner)) => resolve_secret_refs_recursive(inner, resolver, depth + 1),
        Value::Union(_, inner) => resolve_secret_refs_recursive(inner, resolver, depth + 1),
        _ => Ok(()),
    }
}

fn resolve_secret_ref_marker(marker: &str, resolver: &dyn SecretResolver) -> Result<String> {
    let secret_ref = parse_secret_ref(marker)?;
    resolver.resolve(&secret_ref.scheme, &secret_ref.name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::hash_key;
    use std::sync::Arc;

    struct FakeKeyWrapper;

    impl KeyWrapper for FakeKeyWrapper {
        fn wrap(&self, _provider: &str, _kek_locator: &str, dek: &[u8]) -> Result<Vec<u8>> {
            Ok(dek.iter().map(|b| b ^ 0xA5).collect())
        }

        fn unwrap(
            &self,
            _provider: &str,
            _kek_locator: &str,
            wrapped_dek: &[u8],
        ) -> Result<[u8; DEK_LEN]> {
            if wrapped_dek.len() != DEK_LEN {
                return Err(BCSError::Decoding(
                    "FakeKeyWrapper expected 32-byte wrapped DEK".to_string(),
                ));
            }
            let mut out = [0u8; DEK_LEN];
            for (i, b) in wrapped_dek.iter().enumerate() {
                out[i] = b ^ 0xA5;
            }
            Ok(out)
        }
    }

    fn struct_with_password(password: &str) -> Value {
        Value::Struct(vec![(
            "database".to_string(),
            hash_key("database"),
            Value::Struct(vec![(
                "password".to_string(),
                hash_key("password"),
                Value::String(password.to_string()),
            )]),
        )])
    }

    #[test]
    fn protect_and_reveal_roundtrip() {
        let mut value = struct_with_password("s3cret");
        protect_paths(&mut value, &["database.password".to_string()], "master").unwrap();
        assert!(is_protected_marker(match &value {
            Value::Struct(fields) => match &fields[0].2 {
                Value::Struct(inner) => &inner[0].2,
                _ => panic!("expected nested struct"),
            },
            _ => panic!("expected struct"),
        }));

        reveal_paths(&mut value, &["database.password".to_string()], "master").unwrap();

        match value {
            Value::Struct(fields) => match fields[0].2.clone() {
                Value::Struct(inner) => assert_eq!(inner[0].2, Value::String("s3cret".into())),
                other => panic!("unexpected {:?}", other),
            },
            other => panic!("unexpected {:?}", other),
        }
    }

    #[test]
    fn wrong_password_fails() {
        let mut value = struct_with_password("s3cret");
        protect_paths(&mut value, &["database.password".to_string()], "master").unwrap();
        let err =
            reveal_paths(&mut value, &["database.password".to_string()], "wrong").unwrap_err();
        assert!(
            err.to_string().contains("wrong password")
                || err.to_string().contains("Failed to decrypt")
        );
    }

    #[test]
    fn obsolete_unified_sensitive_prefix_is_rejected() {
        let fake = vec![0u8; 40];
        let marker = format!(
            "__bcs_sensitive__:{}",
            base64::engine::general_purpose::STANDARD.encode(&fake)
        );
        let mut value = Value::String(marker);
        let err = reveal_all(&mut value, "any").unwrap_err();
        assert!(err.to_string().contains("Obsolete sensitive marker prefix"));
    }

    #[test]
    fn unknown_sensitive_prefix_is_left_alone() {
        let mut value = Value::String("__bcs_sensitive_nope__:abc".into());
        reveal_all(&mut value, "any").unwrap();
        assert_eq!(value, Value::String("__bcs_sensitive_nope__:abc".into()));
    }

    #[test]
    fn kms_protect_and_reveal_roundtrip() {
        let wrapper = FakeKeyWrapper;
        let mut value = struct_with_password("s3cret");
        protect_paths_kms(
            &mut value,
            &["database.password".to_string()],
            "cmd",
            "alias/test",
            &wrapper,
        )
        .unwrap();
        assert!(is_protected_marker(match &value {
            Value::Struct(fields) => match &fields[0].2 {
                Value::Struct(inner) => &inner[0].2,
                _ => panic!("expected nested struct"),
            },
            _ => panic!("expected struct"),
        }));

        reveal_all_ex(&mut value, None, Some(&wrapper)).unwrap();
        match value {
            Value::Struct(fields) => match fields[0].2.clone() {
                Value::Struct(inner) => assert_eq!(inner[0].2, Value::String("s3cret".into())),
                other => panic!("unexpected {:?}", other),
            },
            other => panic!("unexpected {:?}", other),
        }
    }

    #[test]
    fn mixed_pbkdf2_and_kms_reveal() {
        let wrapper = FakeKeyWrapper;
        let mut value = Value::Struct(vec![
            (
                "password".to_string(),
                hash_key("password"),
                Value::String("pw".into()),
            ),
            (
                "token".to_string(),
                hash_key("token"),
                Value::String("tok".into()),
            ),
        ]);
        protect_paths(&mut value, &["password".to_string()], "master").unwrap();
        protect_paths_kms(&mut value, &["token".to_string()], "cmd", "k1", &wrapper).unwrap();

        reveal_all_ex(&mut value, Some("master"), Some(&wrapper)).unwrap();
        if let Value::Struct(fields) = &value {
            assert_eq!(fields[0].2, Value::String("pw".into()));
            assert_eq!(fields[1].2, Value::String("tok".into()));
        } else {
            panic!("expected struct");
        }
    }

    #[test]
    fn kms_reveal_without_wrapper_fails() {
        let wrapper = FakeKeyWrapper;
        let mut value = struct_with_password("s3cret");
        protect_paths_kms(
            &mut value,
            &["database.password".to_string()],
            "cmd",
            "alias/test",
            &wrapper,
        )
        .unwrap();
        let err = reveal_all_ex(&mut value, None, None).unwrap_err();
        assert!(err.to_string().contains("Key wrapper required"));
    }

    #[test]
    fn missing_path_fails() {
        let mut value = struct_with_password("s3cret");
        let err =
            protect_paths(&mut value, &["database.missing".to_string()], "master").unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let mut value = struct_with_password("s3cret");
        protect_paths(&mut value, &["database.password".to_string()], "master").unwrap();

        if let Value::Struct(fields) = &mut value {
            if let Value::Struct(inner) = &mut fields[0].2 {
                if let Value::String(marker) = &mut inner[0].2 {
                    let mut chars: Vec<u8> = marker.as_bytes().to_vec();
                    let last = chars.len() - 1;
                    chars[last] ^= 0x01;
                    *marker = String::from_utf8_lossy(&chars).into_owned();
                }
            }
        }

        let err = reveal_all(&mut value, "master").unwrap_err();
        assert!(
            err.to_string().contains("decrypt")
                || err.to_string().contains("encoding")
                || err.to_string().contains("payload")
                || err.to_string().contains("UTF")
        );
    }

    #[test]
    fn mask_without_password() {
        let mut value = struct_with_password("s3cret");
        protect_paths(&mut value, &["database.password".to_string()], "master").unwrap();
        mask_all(&mut value).unwrap();
        if let Value::Struct(fields) = &value {
            if let Value::Struct(inner) = &fields[0].2 {
                assert_eq!(inner[0].2, Value::String("[PROTECTED]".into()));
            }
        }
    }

    #[test]
    fn wildcard_path_rejected() {
        let mut value = Value::List(vec![Value::String("x".into())]);
        let err = protect_paths(&mut value, &["[$]".to_string()], "master").unwrap_err();
        assert!(err.to_string().contains("wildcard") || err.to_string().contains("Wild"));
    }

    fn struct_with_secret_ref(scheme: &str, name: &str) -> Value {
        Value::Struct(vec![(
            "api".to_string(),
            hash_key("api"),
            Value::Struct(vec![(
                "token".to_string(),
                hash_key("token"),
                Value::String(format_secret_ref(scheme, name).unwrap()),
            )]),
        )])
    }

    #[test]
    fn parse_secret_ref_env_and_secret() {
        let env_ref = parse_secret_ref("__bcs_secret_ref__:env:DATABASE_PASSWORD").unwrap();
        assert_eq!(
            env_ref,
            SecretRef {
                scheme: "env".into(),
                name: "DATABASE_PASSWORD".into(),
            }
        );

        let secret_ref = parse_secret_ref("__bcs_secret_ref__:secret:api_token").unwrap();
        assert_eq!(
            secret_ref,
            SecretRef {
                scheme: "secret".into(),
                name: "api_token".into(),
            }
        );
    }

    #[test]
    fn parse_secret_ref_accepts_future_provider_schemes() {
        let vault_ref =
            parse_secret_ref("__bcs_secret_ref__:vault:secret/data/db#password").unwrap();
        assert_eq!(vault_ref.scheme, "vault");
        assert_eq!(vault_ref.name, "secret/data/db#password");
    }

    #[test]
    fn parse_secret_ref_rejects_invalid_scheme() {
        let err = parse_secret_ref("__bcs_secret_ref__:Env:db").unwrap_err();
        assert!(err.to_string().contains("Invalid secret reference scheme"));
    }

    #[test]
    fn mask_secret_refs_without_resolve() {
        let mut value = struct_with_secret_ref("env", "API_TOKEN");
        assert!(is_secret_ref_marker(match &value {
            Value::Struct(fields) => match &fields[0].2 {
                Value::Struct(inner) => &inner[0].2,
                _ => panic!("expected nested struct"),
            },
            _ => panic!("expected struct"),
        }));

        mask_secret_refs(&mut value).unwrap();
        if let Value::Struct(fields) = &value {
            if let Value::Struct(inner) = &fields[0].2 {
                assert_eq!(inner[0].2, Value::String("[SECRET_REF]".into()));
            }
        }
    }

    #[test]
    fn resolve_secret_refs_from_env() {
        std::env::set_var("BCS_TEST_API_TOKEN", "tok_from_env");
        let mut value = struct_with_secret_ref("env", "BCS_TEST_API_TOKEN");
        resolve_secret_refs(&mut value, &ResolverRegistry::with_env()).unwrap();
        if let Value::Struct(fields) = &value {
            if let Value::Struct(inner) = &fields[0].2 {
                assert_eq!(inner[0].2, Value::String("tok_from_env".into()));
            }
        }
        std::env::remove_var("BCS_TEST_API_TOKEN");
    }

    #[test]
    fn resolve_secret_scheme_uses_default_provider() {
        std::env::set_var("BCS_TEST_SECRET_NAME", "tok_via_secret_scheme");
        let mut value = struct_with_secret_ref("secret", "BCS_TEST_SECRET_NAME");
        resolve_secret_refs(&mut value, &ResolverRegistry::with_env()).unwrap();
        if let Value::Struct(fields) = &value {
            if let Value::Struct(inner) = &fields[0].2 {
                assert_eq!(inner[0].2, Value::String("tok_via_secret_scheme".into()));
            }
        }
        std::env::remove_var("BCS_TEST_SECRET_NAME");
    }

    #[test]
    fn resolve_secret_scheme_fails_without_default() {
        let mut registry = ResolverRegistry::new();
        registry.register("env", Arc::new(EnvSecretResolver));
        // no set_default_scheme

        let mut value = struct_with_secret_ref("secret", "ANY");
        let err = resolve_secret_refs(&mut value, &registry).unwrap_err();
        assert!(err.to_string().contains("no default secret provider"));
    }

    #[test]
    fn resolve_secret_refs_missing_env_fails() {
        std::env::remove_var("BCS_TEST_MISSING_SECRET");
        let mut value = struct_with_secret_ref("env", "BCS_TEST_MISSING_SECRET");
        let err = resolve_secret_refs(&mut value, &ResolverRegistry::with_env()).unwrap_err();
        assert!(err.to_string().contains("not set"));
    }

    #[test]
    fn resolve_unregistered_scheme_fails() {
        let mut value = Value::String(format_secret_ref("vault", "secret/data/db").unwrap());
        let err = resolve_secret_refs(&mut value, &ResolverRegistry::with_env()).unwrap_err();
        assert!(err.to_string().contains("no provider registered"));
    }

    #[test]
    fn resolve_with_fake_resolver_via_registry() {
        struct FakeResolver;
        impl SecretResolver for FakeResolver {
            fn resolve(&self, scheme: &str, locator: &str) -> Result<String> {
                Ok(format!("{}={}", scheme, locator))
            }
        }

        let mut registry = ResolverRegistry::new();
        registry.register("vault", Arc::new(FakeResolver));

        let mut value = Value::String(format_secret_ref("vault", "kv/db").unwrap());
        resolve_secret_refs(&mut value, &registry).unwrap();
        assert_eq!(value, Value::String("vault=kv/db".into()));
    }

    #[test]
    fn registry_for_provider_env_only_in_core() {
        assert!(registry_for_provider("env").is_ok());
        match registry_for_provider("vault") {
            Ok(_) => panic!("expected unknown provider error"),
            Err(err) => assert!(err.to_string().contains("Unknown secret provider")),
        }
    }

    #[test]
    fn mask_sensitive_fields_masks_both() {
        let mut value = Value::Struct(vec![
            (
                "password".to_string(),
                hash_key("password"),
                Value::String("plain".into()),
            ),
            (
                "token".to_string(),
                hash_key("token"),
                Value::String(format_secret_ref("env", "API_TOKEN").unwrap()),
            ),
        ]);
        protect_paths(&mut value, &["password".to_string()], "master").unwrap();
        mask_sensitive_fields(&mut value).unwrap();
        if let Value::Struct(fields) = &value {
            assert_eq!(fields[0].2, Value::String("[PROTECTED]".into()));
            assert_eq!(fields[1].2, Value::String("[SECRET_REF]".into()));
        }
    }
}

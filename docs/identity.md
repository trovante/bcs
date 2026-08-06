# Password protect, KMS protect, secret refs, and ephemeral identity

BCS offers independent overlays for sensitive data, plus a deployment pattern
that avoids long-lived credentials. See also [`secrets.md`](secrets.md).

## Choose a pattern

| Pattern | Marker prefix | When to use |
|---------|---------------|-------------|
| **Password protect (KDF)** | `__bcs_sensitive_pbkdf2__:` | Offline / shared password; no cloud KMS |
| **KMS protect** | `__bcs_sensitive_kms__:` | In-file ciphertext; DEK wrap via host or native KMS |
| **Secret reference** | `__bcs_secret_ref__:` | Value stays in env/Vault/cloud SM |

Password (`pbkdf2`) and KMS markers **may mix in one file**. Secret refs are
independent of both. Prefer refs + ephemeral identity for production when the
secret should not live in the file at all.

## Sensitive markers (distinct prefixes)

There is no versioned `v1`/`v2` payload. Schemes are named by **prefix**:

| Scheme | Prefix | Key material |
|--------|--------|--------------|
| `pbkdf2` | `__bcs_sensitive_pbkdf2__:` | Password → PBKDF2-HMAC-SHA256 |
| `kms` | `__bcs_sensitive_kms__:` | Random DEK; wrap/unwrap via `KeyWrapper` |

The obsolete unified prefix `__bcs_sensitive__:` is rejected.

### `__bcs_sensitive_pbkdf2__:` (KDF)

Password-based field encryption:

1. Derive a 32-byte key with **PBKDF2-HMAC-SHA256** (default **120 000**
   iterations; iteration count is stored in the payload so older markers remain
   decryptable if the default changes later).
2. Encrypt the JSON-serialized field value with **AES-256-GCM**.
3. Store Base64 over:

```text
iterations:u32_le | salt:16 | nonce:12 | ciphertext
```

CLI: `--scheme pbkdf2` (default) with `--password` / `--password-env`, or
encode-time `--protect-scheme pbkdf2` with `--protect-password`.

### `__bcs_sensitive_kms__:` (KMS)

KMS-wrapped field encryption (envelope encryption):

1. Generate a random 32-byte **DEK**.
2. Encrypt the field value with **AES-256-GCM** under that DEK.
3. **Wrap** the DEK with the host/`bcs-secrets` `KeyWrapper` (cloud KMS or
   Vault/OpenBao Transit).
4. Store Base64 over:

```text
provider_len:u8 | provider
| kek_locator_len:u16_le | kek_locator
| wrapped_dek_len:u16_le | wrapped_dek
| nonce:12 | ciphertext
```

CLI: `--scheme kms --kms-provider … --kms-key …`. Decode with `--unwrap-kms`
(and optional `--kms-provider`).

## Native KMS / Transit wrappers (`bcs-secrets`)

Feature-gated `KeyWrapper` backends (same Cargo features as secret providers):

| Provider (`--kms-provider`) | Feature | KEK locator |
|-----------------------------|---------|-------------|
| `aws` | `secrets-aws` | KMS key id / ARN / `alias/…` |
| `azure` | `secrets-azure` | Key Vault key URI or `vault/key` |
| `gcp` | `secrets-gcp` | Cloud KMS cryptoKey resource name |
| `vault` / `openbao` | `secrets-vault` | Transit key name |
| `cmd` | _(always)_ | any; uses `BCS_KMS_*_CMD` |

```bash
# Native AWS KMS (build CLI with --features secrets-aws)
bcs protect config.bcs --paths database.password \
  --scheme kms --kms-provider aws --kms-key alias/app-key

bcs decode config.protected.bcs --unwrap-kms --kms-provider aws

# Encode-time protect with the same scheme
bcs encode examples/secure-config.json -o config.kms.bcs \
  --protect-paths database.password \
  --protect-scheme kms --kms-provider aws --kms-key alias/app-key

# External command (no cloud SDK in-process)
export BCS_KMS_WRAP_CMD='…' BCS_KMS_UNWRAP_CMD='…'
bcs protect … --scheme kms --kms-provider cmd --kms-key alias/x
bcs decode … --unwrap-kms --kms-provider cmd
```

When `--unwrap-kms` is set without `--kms-provider`, the CLI tries all native
wrappers available in the build (plus `cmd` if configured), matching the
provider label stored in each marker.

FFI/Python accept host wrap/unwrap callables (`bcs_protect_json_ex`,
`bcs_decode_to_json_ex`).

## Ephemeral identity

Prefer OIDC/IAM/Managed Identity over long-lived tokens when calling secret
providers or KMS wrap/unwrap. See [`secrets.md`](secrets.md).

## Related

- [Secret providers](secrets.md)
- [Compatibility policy](compatibility-policy.md)
- [API reference](api-reference.md)

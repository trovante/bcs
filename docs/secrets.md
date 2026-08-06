# Secret providers

BCS secret references (`__bcs_secret_ref__:<scheme>:<locator>`) resolve at decode
time when you pass `--resolve-secrets`. Providers are selected with
`--secret-provider` or `BCS_SECRET_PROVIDER` (default: `env`).

Logical `secret:NAME` refs remap to the selected provider’s default scheme.

For **when to use password protect vs refs vs cloud/workload identity**, see
[`identity.md`](identity.md).

## Build features (CLI)

| Feature | Provider | Default |
|---------|----------|---------|
| _(always)_ | `env` | yes |
| `secrets-vault` | `vault` (+ `openbao` scheme) | **yes** |
| `secrets-openbao` | `openbao` (alias of vault client) | via vault |
| `secrets-aws` | `aws` | no |
| `secrets-azure` | `azure` | no |
| `secrets-gcp` | `gcp` | no |
| `secrets-doppler` | `doppler` | no |
| `secrets-infisical` | `infisical` | no |
| `secrets-akeyless` | `akeyless` | no |
| `secrets-bitwarden` | `bitwarden` | no |
| `secrets-onepassword` | `op` / `onepassword` | no |
| `secrets-kubernetes` | `k8s` / `kubernetes` | no |
| `secrets-all` | all of the above | no |

```bash
cargo build -p bcs-cli --release
cargo build -p bcs-cli --release --features secrets-all
```

## Providers

### `env`

Locator: env var name. Auth: process environment.

Prefer injecting secrets via the orchestrator (Kubernetes Secrets, CI masked
vars, cloud parameter store → env) rather than baking values into the BCS file.

### `vault` / `openbao`

Locator: API path after `/v1/`, optional `#field` (KV). Example: `secret/data/myapp#password`.

| | Vault | OpenBao |
|--|-------|---------|
| Address | `VAULT_ADDR` | `BAO_ADDR` / `OPENBAO_ADDR` (fallback `VAULT_ADDR`) |
| Token | `VAULT_TOKEN` | `BAO_TOKEN` / `OPENBAO_TOKEN` (fallback `VAULT_TOKEN`) |
| Namespace | `VAULT_NAMESPACE` | `BAO_NAMESPACE` / `VAULT_NAMESPACE` |
| Timeout | `BCS_VAULT_TIMEOUT_SECS` | `BCS_OPENBAO_TIMEOUT_SECS` (fallback vault) |

**Preferred auth:** Kubernetes / JWT-OIDC / AppRole / cloud IAM → short-lived
token in `VAULT_TOKEN`. Avoid long-lived root tokens. See [identity.md](identity.md).

### `aws`

Locator: secret name/ARN, optional `#json_field`. Env: `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, optional `AWS_SESSION_TOKEN`, `AWS_REGION` / `AWS_DEFAULT_REGION`, `BCS_AWS_SECRET_TIMEOUT_SECS`.

**Preferred auth:** IAM role / IRSA / temporary session credentials—not permanent
access keys. See [identity.md](identity.md).

### `azure`

Locator: full Key Vault URI, `vaultName/secretName`, or bare name + `AZURE_KEY_VAULT_URL`. Auth: `AZURE_ACCESS_TOKEN` **or** `AZURE_TENANT_ID` + `AZURE_CLIENT_ID` + `AZURE_CLIENT_SECRET`. Timeout: `BCS_AZURE_TIMEOUT_SECS`.

**Preferred auth:** Managed Identity / Workload Identity → short-lived
`AZURE_ACCESS_TOKEN`. See [identity.md](identity.md).

### `gcp`

Locator: `projects/.../secrets/.../versions/...` or short name + `GOOGLE_CLOUD_PROJECT` / `GCP_PROJECT`. Auth: `GOOGLE_ACCESS_TOKEN` / `GCP_ACCESS_TOKEN`. Timeout: `BCS_GCP_TIMEOUT_SECS`.

**Preferred auth:** Workload Identity / ADC → short-lived access token. See
[identity.md](identity.md).

### `doppler`

Locator: `SECRET_NAME` (needs `DOPPLER_PROJECT` + `DOPPLER_CONFIG`) or `project/config/SECRET_NAME`. Auth: `DOPPLER_TOKEN`. Optional: `DOPPLER_API_URL`, `BCS_DOPPLER_TIMEOUT_SECS`.

Prefer short-lived service tokens over personal tokens.

### `infisical`

Locator: secret name. Auth: `INFISICAL_TOKEN` + `INFISICAL_PROJECT_ID` / `INFISICAL_WORKSPACE_ID`. Optional: `INFISICAL_API_URL`, `INFISICAL_ENVIRONMENT` (default `dev`), `INFISICAL_SECRET_PATH` (default `/`), `BCS_INFISICAL_TIMEOUT_SECS`.

### `akeyless`

Locator: secret path/name (e.g. `/prod/db`). Auth: `AKEYLESS_TOKEN` **or** `AKEYLESS_ACCESS_ID` + `AKEYLESS_ACCESS_KEY`. Optional: `AKEYLESS_API_URL`, `BCS_AKEYLESS_TIMEOUT_SECS`.

Prefer OIDC / cloud identity federation where available.

### `bitwarden`

Locator: secret UUID. Auth: `BWS_ACCESS_TOKEN` / `BITWARDEN_ACCESS_TOKEN` **or** `BITWARDEN_CLIENT_ID` + `BITWARDEN_CLIENT_SECRET`. Optional: `BITWARDEN_API_URL`, `BITWARDEN_IDENTITY_URL`, `BCS_BITWARDEN_TIMEOUT_SECS`.

### `op` / `onepassword` (feature `secrets-onepassword`)

Locator: `op://vault/item/field` or `vault/item/field`. Auth: 1Password CLI (`op read`). Prefer short-lived session tokens; never log resolved values.

Providers shell out via argv only (no `sh -c`). Unit tests inject a `CommandRunner` fake so CI does not need the real `op` binary.

### `k8s` / `kubernetes` (feature `secrets-kubernetes`)

Locator: `namespace/name/key` or `name/key` (namespace from `BCS_K8S_NAMESPACE`, default `default`). Resolves via `kubectl get secret`. Prefer in-cluster workload identity in production.

Same injectable `CommandRunner` as 1Password; tests cover base64 decode and kubectl failures without a cluster.

## Native KMS / Transit (field protect)

Separate from secret-ref resolvers: wrap/unwrap DEKs for `__bcs_sensitive_kms__:`
markers via [`KeyWrapper`](../core/src/security.rs). Password protect
(`__bcs_sensitive_pbkdf2__:`) does not use these backends.

| `--kms-provider` | Feature | Notes |
|------------------|---------|-------|
| `aws` | `secrets-aws` | AWS KMS Encrypt/Decrypt |
| `azure` | `secrets-azure` | Key Vault wrapkey/unwrapkey |
| `gcp` | `secrets-gcp` | Cloud KMS encrypt/decrypt |
| `vault` / `openbao` | `secrets-vault` | Transit encrypt/decrypt |
| `cmd` | always | `BCS_KMS_WRAP_CMD` / `BCS_KMS_UNWRAP_CMD` |

Payload layout, coexistence with `pbkdf2`, and CLI examples:
[identity.md](identity.md).

## FFI / language bindings

- C: `bcs_decode_to_json` always masks refs; use `bcs_decode_to_json_ex` with a
  `bcs_secret_resolve_fn` callback (return values via `bcs_strdup`).
- Python: `decode_to_json(..., resolve_secrets=fn)` where `fn(scheme, locator)`.
- CLI `--stream` applies the same password mask/reveal and secret mask/resolve
  as full decode.

## Safety notes

- Decode without `--resolve-secrets` (or without an FFI/Python callback) never
  contacts remote providers (`[SECRET_REF]`).
- Do not store provider credentials inside the BCS file.
- Prefer ephemeral identity over static tokens ([identity.md](identity.md)).
- Error messages avoid echoing secret values.

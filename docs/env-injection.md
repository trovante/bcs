# Environment injection

Inject configuration from a `.bcs` file into a **process environment** — no `.env` files are generated or written.

## Mental model

```text
JSON / YAML / TOML  →  bcs encode  →  file.bcs
                                         │
                                         ▼
                              bcs run … -- <command>
                                         │
                    decode → reveal protect / resolve refs
                                         │
                         flatten keys  or  --json-env
                                         │
                              child process env overlay
```

- **Source of truth:** nested document packaged as `.bcs`
- **Runtime:** OS process boundary (`Command::env`), not a language library
- **App contract:** normal env vars (`process.env`, `os.environ`, `std::env`, …) or one JSON blob via `--json-env`

Injection is **language-agnostic**. The same command works for Node, Bun, Python, Go, Rust, shell scripts, or any binary that reads the environment. No `require('dotenv')`, no framework plugins, no language-specific BCS SDK for this path.

## Quick recipes

### Default: run any command with flattened env

```bash
bcs run config.bcs -- ./my-app
bcs run config.bcs -- node server.js
bcs run config.bcs -- bun run start
bcs run config.bcs -- python -m myapp
```

With protect password and secret refs:

```bash
bcs run config.bcs \
  --password-env BCS_PROTECT_PASSWORD \
  --resolve-secrets \
  -- ./my-app
```

### Preview keys (redacted)

```bash
bcs run config.bcs --dry-run
```

Sensitive values print as `[REDACTED]` (schema `sensitive_paths` plus key-name heuristics). Use this to check key names, not as a secret dump. Agents should prefer `bcs schema --agent-safe` for shape without values.

### Subtree only

```bash
bcs run config.bcs --path database -- ./my-app
```

Exports only the `database` object. Flattening starts at that subtree with an empty prefix, so `database.host` becomes `HOST` (not `DATABASE__HOST`).

### Allowlist paths

```bash
bcs run config.bcs --only database.host,api.port -- ./my-app
```

### Namespace with a prefix

```bash
bcs run config.bcs --prefix APP_ -- ./my-app
# database.host → APP_DATABASE__HOST
```

### Nested-aware apps: one JSON env var

```bash
bcs run config.bcs --json-env APP_CONFIG -- ./my-app
```

Sets `APP_CONFIG` to the full JSON document (or the `--path` subtree). By default flattened keys are also exported; use `--export-env=false` if you only want the JSON var.

### Shell helper (still no `.env` file)

```bash
eval "$(bcs env config.bcs --prefix APP_)"
```

Prints `KEY='value'` lines for the **current shell**. Sensitive values are redacted by default; `--allow-sensitive` prints real values (operator-only; warns on stderr). Prefer `bcs run` for injecting secrets into a child so they never hit the terminal.

### CI / Kubernetes

Prefer secret references in the `.bcs` plus orchestrator-injected provider credentials. Resolve at start:

```bash
bcs run config.bcs --resolve-secrets -- ./my-app
```

See [secrets.md](secrets.md) and [cli-security.md](cli-security.md).

## Flattening contract

| Shape | Env key rule |
|-------|----------------|
| Nested object / map | `PARENT__CHILD` (double underscore), keys uppercased, non-alphanumeric → `_` |
| List | Numeric segments: `ITEMS__0__NAME` |
| Scalars | String forms (`true`/`false`, decimal numbers); bytes → standard base64 |
| Null / empty optional | Omitted |

Examples:

| Document path | Env key |
|---------------|---------|
| `database.host` | `DATABASE__HOST` |
| `api.rate-limit` | `API__RATE_LIMIT` |
| `items[0].name` | `ITEMS__0__NAME` |

**Precedence:** BCS overlay **overwrites** existing environment variables with the same name when spawning the child.

**Dry-run / `bcs env` redaction:** schema `sensitive_paths` (prefix match on dotted path) plus heuristics on key names containing `password`, `secret`, `token`, or ending in `_key`.

## vs dotenvx / Varlock

| | dotenvx | Varlock | BCS |
|--|---------|---------|-----|
| Source of truth | Flat `.env` (+ encrypt in place) | `.env.schema` + env files | Nested document in `.bcs` |
| Runtime | Decrypt → inject env | Schema load → inject | Decode / reveal → inject |
| Multi-env | `.env.production` + key suffix | First-class | Separate `.bcs` per stage |
| Secrets | File ciphertext + private key | Backends + schema | Field `protect` / secret refs / KMS |
| App contract | `process.env.FOO` | Typed env | Flat env and/or `--json-env` |

BCS borrows the useful bit (`run -- cmd`, redacted preview, app never sees ciphertext) and does **not** become an encrypted multi-env `.env` platform. Longer comparison: [comparison-varlock-rx.md](comparison-varlock-rx.md).

## Non-goals

- Generating or committing `.env` / `.env.production` files
- dotenvx public/private key encryption of `.env`
- Multi-file env cascading or auto file selection from private-key name
- Language-specific loaders or framework plugins (Next / Vite / etc.)
- Replacing Varlock for local `.env` authoring

## Related CLI

| Command | Role |
|---------|------|
| `bcs run <file.bcs> -- <cmd>…` | Primary: inject into child process env and exec |
| `bcs env <file.bcs>` | Print `KEY='value'` for shell `eval` (redacted by default) |
| `bcs run --dry-run` | Preview keys without exec |

Flags shared by `run` / `env` where applicable: `--path`, `--prefix`, `--only`, `--json-env`, `--resolve-secrets`, `--password` / `--password-env`, `--unwrap-kms`.

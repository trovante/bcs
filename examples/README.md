# Examples

Sample configs for CLI demos, docs, and tests.

| File | Purpose |
|---|---|
| `test.json` / `.yaml` / `.toml` | Small multi-format smoke fixtures |
| `test-nested.json` / `.yaml` / `.toml` | Nested medium fixtures (also bench medium) |
| `secure-config.json` | Field-protection / password + secret-ref demos |
| `app-settings.json` | Richer app config (core integration + benches) |
| `kubernetes-deployment.json` | Larger nested document (core integration) |

All fixtures live here (workspace root). Crate tests resolve them via `CARGO_MANIFEST_DIR/../examples`.

Ephemeral encode outputs for local experiments should go under gitignored `tmp/` (see root README), not under this folder.

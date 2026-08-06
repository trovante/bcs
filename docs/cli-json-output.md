# CLI JSON Output Contract

This document defines the machine-readable output contract for CLI commands that support `--json`.

## Scope

Commands covered:

- `bcs validate --json`
- `bcs inspect --json`
- `bcs benchmark --json`
- `bcs protect --json`
- `bcs reindex --json`

`benchmark` also supports `--mode path-hot` for repeated path-query focus.

The goal is to provide stable keys and value types that are easy to consume with `jq` and CI scripts.

## Stability Guidelines

- Existing top-level keys should remain stable in patch releases.
- New keys may be added in minor releases.
- Removed/renamed keys should be documented in `CHANGELOG.md`.

## 1) `validate --json`

Example shape:

```json
{
  "ok": true,
  "error_count": 0,
  "warning_count": 0,
  "errors": [],
  "warnings": [],
  "fail_on_sensitive_plaintext": false
}
```

Fields:

- `ok` (`boolean`): validation status (false if schema errors, or sensitive plaintext when `--fail-on-sensitive-plaintext`)
- `error_count` (`number`): number of hard failures
- `warning_count` (`number`): sensitive-plaintext findings when not failing on them
- `errors` (`array`): list of validation errors
  - item fields:
    - `path` (`string`): failing path, `"<root>"` when root-level
    - `message` (`string`): validation error text
    - `kind` (`string`, optional): `"schema"` or `"sensitive_plaintext"`
- `warnings` (`array`): sensitive-plaintext findings (same item shape as errors)
- `fail_on_sensitive_plaintext` (`boolean`): whether the fail flag was set

Default policy: sensitive plaintext is a **warning**. Pass `--fail-on-sensitive-plaintext` to treat it as failure.

Useful `jq` snippets:

```bash
# Return non-zero if validation failed
bcs validate config.bcs --json | jq -e '.ok == true' >/dev/null

# Print all errors
bcs validate config.bcs --json | jq -r '.errors[] | "\(.path): \(.message)"'
```

Agent-safe schema export (not `--json`, but stdout/file JSON): see [agent-schema.md](agent-schema.md) (`bcs schema --agent-safe`).

## 2) `inspect --json`

Example shape:

```json
{
  "file": "config.bcs",
  "metadata": { ... },
  "header": { ... },
  "schema": { ... },
  "index_table": { ... }
}
```

Fields:

- `file` (`string`)
- `metadata` (`object`)
  - `version_major` (`number`)
  - `version_minor` (`number`)
  - `compressed` (`boolean`)
  - `data_compressed` (`boolean`)
  - `ai_metadata` (`boolean`) — reserved header bit `0x0002` as observed on disk (always `false` on newly written files; readers ignore)
  - `semantic_size` (`number`)
  - `index_size` (`number`)
  - `data_size` (`number`)
  - `total_size` (`number`)
- `header` (`object`)
  - `magic` (`number`)
  - `flags` (`number`)
  - `checksum` (`number`)
  - `semantic_offset` (`number`)
  - `index_offset` (`number`)
  - `data_offset` (`number`)
- `schema` (`object`)
  - `ok` (`boolean`)
  - when `ok=true`:
    - `version` (`string`)
    - `root` (`string`)
    - `type_count` (`number`)
    - `constraint_count` (`number`)
    - `documentation_count` (`number`)
    - `ai_tag_count` (`number`)
    - `types` (`object|null`) (present when `--verbose`)
  - when `ok=false`:
    - `error` (`string`)
- `index_table` (`object`)
  - `ok` (`boolean`)
  - when `ok=true`:
    - `stats.entry_count` (`number`)
    - `stats.bucket_count` (`number`)
    - `stats.load_factor` (`number`)
    - `stats.collision_rate` (`number`)
  - when `ok=false`:
    - `error` (`string`)

Useful `jq` snippets:

```bash
# Check whether data layer is compressed
bcs inspect config.bcs --json | jq '.metadata.data_compressed'

# Read index entry count
bcs inspect config.bcs --json | jq '.index_table.stats.entry_count'
```

## 3) `benchmark --json`

Example shape:

```json
{
  "file": "config.bcs",
  "runs": 5,
  "mode": "full",
  "bcs": { ... },
  "compare": null
}
```

When `--compare` is used:

```json
{
  "file": "config.bcs",
  "runs": 5,
  "mode": "full",
  "bcs": { ... },
  "compare": {
    "file": "config.json",
    "results": { ... },
    "comparison": { ... }
  }
}
```

In `--mode path-hot`, `mode` is `"path-hot"` and load/decode/random-access metrics are zeroed intentionally to keep the run focused on repeated path-query latency.

`bcs` / `compare.results` fields:

- `file_size` (`number`)
- `load_time_p50_ns` (`number`)
- `load_time_p95_ns` (`number`)
- `decode_time_p50_ns` (`number`)
- `decode_time_p95_ns` (`number`)
- `random_access_avg_ns` (`number`)
- `random_access_samples` (`number`)
- `path_get_simple_p95_ns` (`number`)
- `path_get_simple_samples` (`number`)
- `path_get_deep_p95_ns` (`number`)
- `path_get_deep_samples` (`number`)
- `path_get_wildcard_p95_ns` (`number`)
- `path_get_wildcard_samples` (`number`)
- `path_get_hot_p95_ns` (`number`): repeated-query hot-loop p95 latency
- `path_get_hot_samples` (`number`)
- `memory_usage_bytes` (`number`)
- `runs` (`number`)

`compare.comparison` fields:

- `load_speedup_x` (`number`)
- `decode_speedup_x` (`number`)
- `size_ratio_percent` (`number`)

Useful `jq` snippets:

```bash
# Get BCS decode p95
bcs benchmark config.bcs --json | jq '.bcs.decode_time_p95_ns'

# Get repeated path-query hot-loop p95
bcs benchmark config.bcs --mode path-hot --json | jq '.bcs.path_get_hot_p95_ns'

# Get comparison decode speedup
bcs benchmark config.bcs --compare config.json --json | jq '.compare.comparison.decode_speedup_x'
```

## 4) `protect --json`

Example shape:

```json
{
  "ok": true,
  "file": "config.bcs",
  "output": "config.protected.bcs",
  "path_count": 2,
  "input_size": 201,
  "output_size": 209,
  "compression_ratio_percent": 104.0
}
```

Fields:

- `ok` (`boolean`): command status
- `file` (`string`): input BCS path
- `output` (`string`): output BCS path
- `path_count` (`number`): number of sensitive paths protected
- `input_size` (`number`): input size in bytes
- `output_size` (`number`): output size in bytes
- `compression_ratio_percent` (`number`): `(output_size / input_size) * 100`

Useful `jq` snippets:

```bash
# Validate protection succeeded
bcs protect config.bcs --paths database.password --password "$PASS" --json | jq -e '.ok == true' >/dev/null

# Check number of protected paths
bcs protect config.bcs --paths database.password,api.token --password "$PASS" --json | jq '.path_count'
```

## 5) `reindex --json`

Example shape (write mode):

```json
{
  "ok": true,
  "file": "compact.bcs",
  "output": "compact.reindexed.bcs",
  "dry_run": false,
  "input_size": 363,
  "output_size": 642,
  "ratio_percent": 176.9,
  "input_sections": { ... },
  "output_sections": { ... },
  "options": {
    "add_schema": false,
    "compress_data": false
  }
}
```

Example shape (dry-run mode):

```json
{
  "ok": true,
  "file": "compact.bcs",
  "output": null,
  "dry_run": true,
  "input_size": 363,
  "projected_output_size": 642,
  "ratio_percent": 176.9,
  "input_sections": { ... },
  "projected_output_sections": { ... },
  "options": {
    "add_schema": true,
    "compress_data": false
  }
}
```

Section object fields:

- `semantic_size` (`number`)
- `index_size` (`number`)
- `data_size` (`number`)
- `total_size` (`number`)

Useful `jq` snippets:

```bash
# Get projected output size without writing file
bcs reindex compact.bcs --dry-run --json | jq '.projected_output_size'

# Check whether schema embedding was enabled
bcs reindex compact.bcs --add-schema --json | jq '.options.add_schema'
```

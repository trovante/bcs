# CLI Usage

Complete reference for the `bcs` command-line tool. For a short overview, see the [root README](../README.md).

Install from source:

```bash
cargo install --path cli
```

During development:

```bash
cargo run -p bcs-cli -- --help
# or, after cargo build:
./target/debug/bcs --help
```

## Core commands

```bash
# Encode (output defaults to same folder with .bcs extension)
bcs encode config.json

# Encode to explicit output path
bcs encode config.json -o config.bcs

# Decode
bcs decode config.bcs -o config.json

# Decode with progress logs
bcs decode config.bcs -o config.json --verbose

# Partial query (requires index table; data-layer compression is decompressed once and cached)
bcs decode config.bcs --path server.host
bcs decode config.bcs --path server.host --verbose   # prints access=indexed|walk

# Agent-safe schema (no values)
bcs schema --agent-safe config.bcs

# Scan sources / .bcs for leaks
bcs scan examples/ --json
# CI / pre-commit helper: scripts/scan-ci-example.sh

# Run a command with config as env (language-agnostic; no .env files)
# See docs/env-injection.md
bcs run config.bcs --dry-run
bcs run config.bcs -- ./my-app
bcs run config.bcs -- node server.js
bcs run config.bcs --prefix APP_ --only database.host -- ./my-app
bcs env config.bcs                          # KEY='value' for eval (redacted)
eval "$(bcs env config.bcs --prefix APP_)"

# Segment show / tree inspect
bcs show config.bcs database host
bcs inspect --tree config.bcs
bcs dump --format debug-tree config.bcs

# Validate
bcs validate config.bcs

# Validate as JSON
bcs validate config.bcs --json
bcs validate config.bcs --fail-on-sensitive-plaintext

# Inspect metadata
bcs inspect config.bcs --verbose

# Inspect as JSON
bcs inspect config.bcs --json

# Benchmark
bcs benchmark config.bcs --compare config.json

# Benchmark as JSON
bcs benchmark config.bcs --compare config.json --json

# Benchmark with custom run count (percentiles over N runs)
bcs benchmark config.bcs --compare config.json --runs 10

# Benchmark repeated path-query hot loop
bcs benchmark config.bcs --mode path-hot --runs 20

# Benchmark repeated path-query hot loop as JSON
bcs benchmark config.bcs --mode path-hot --runs 20 --json

# Rebuild an indexed file from a compact BCS
bcs reindex compact.bcs -o indexed.bcs

# Rebuild with default output path (<input>.reindexed.bcs)
bcs reindex compact.bcs

# Rebuild indexed file and include semantic layer
bcs reindex compact.bcs -o indexed-with-schema.bcs --add-schema

# Rebuild indexed file with data compression
bcs reindex compact.bcs -o indexed-compressed.bcs --compress-data

# Preview reindex section changes without writing file
bcs reindex compact.bcs --dry-run --add-schema

# Reindex JSON output (CI-friendly)
bcs reindex compact.bcs --json
```

## Size profiles

```bash
# Default profile (semantic + index + data)
bcs encode config.json -o config.default.bcs

# Compact profile (data-focused, lower overhead)
bcs encode config.json -o config.compact.bcs --compact

# Compact + optional data compression (applies only when beneficial)
bcs encode config.json -o config.compact.compressed.bcs --compact --compress-data

# Keep metadata/index while still allowing data compression
bcs encode config.json -o config.default.compressed.bcs --compress-data

# Opt-in structural dedup for repetitive keys/strings
bcs encode config.json -o config.dedup.bcs --dedup all

# Dedup + nested indexes for large structs/maps
bcs encode config.json -o config.dedup-indexed.bcs --dedup strings --index-maps-over 32
```

Notes:

- `--compress-data` uses smart fallback: if compressed data is not smaller, raw data is kept.
- When data-layer compression is active, path lookup decompresses once and caches the logical layer (see [adr/compress-path.md](adr/compress-path.md)).
- `--dedup` is off by default. Modes: `keys` (field/map keys), `strings` (leaf strings), `all`. Thresholds: `--dedup-min-repeats` (default 2), `--dedup-min-length` (default 4).
- `--index-maps-over N` registers nested `parent.child` index entries for structs with ≥ N fields (maps register the path at the map root).
- `reindex` is useful to add path-query capability back to compact files.

## Path syntax

Mixed nesting depth:

- Dot notation for object fields: `root.child.key`
- Bracket notation for array indices: `items[0]`
- Combined nesting examples:
  - `services[0].routes[1].method`
  - `services[0].routes[1].paths[0]`
  - `services[1].ports[0]`

Wildcard path syntax:

- Dot wildcard for arrays: `.$.`
  - Example: `services.$.routes.$.paths`
- Bracket wildcard for arrays: `[$]`
  - Example: `services[$].routes[$].paths`

Wildcard queries return a list of all matched values.

When `--path-flatten` is used, nested list matches are flattened into a single list.

Sample nested fixtures:

- `examples/test-nested.json`
- `examples/test-nested.yaml`
- `examples/test-nested.toml`

## JSON in CI (`jq` quick checks)

```bash
# Fail pipeline if validation fails
bcs validate config.bcs --json | jq -e '.ok == true' >/dev/null

# Read key inspect fields
bcs inspect config.bcs --json | jq '{total_size: .metadata.total_size, data_compressed: .metadata.data_compressed}'

# Track benchmark decode p95 (full mode)
bcs benchmark config.bcs --json | jq '.bcs.decode_time_p95_ns'

# Track benchmark path hot-loop p95
bcs benchmark config.bcs --mode path-hot --json | jq '.bcs.path_get_hot_p95_ns'

# Fail if protect command did not succeed
bcs protect config.bcs --paths database.password --password "$PASS" --json | jq -e '.ok == true' >/dev/null

# Preview reindex impact without writing output
bcs reindex compact.bcs --dry-run --json | jq '{projected_output_size, projected_output_sections}'
```

Machine-readable field contracts: [CLI JSON Output](cli-json-output.md).

## Related

- [Field protection & secret refs (CLI)](cli-security.md)
- [Identity strategy](identity.md)
- [Secret providers](secrets.md)
- [Measured benchmarks](benchmarks.md)

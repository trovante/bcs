# Benchmarks

Read-performance regression assets and recorded measurement snapshots.

## Layout

| Path | Checked in? | Purpose |
|---|---|---|
| `baseline/*.json` | Yes | Gate baselines (KPI + provenance metadata) |
| `measured-snapshot.json` | Yes | Docs-facing snapshot (sizes, compares, profile summary) |
| `measured-readme-fragment.md` | Yes | Human fragment generated for docs updates (`docs/benchmarks.md`) |
| `tmp/` | No (gitignored) | Generated fixtures: `small.bcs`, `medium.bcs`, `large.json`, `large.bcs` |
| `current/` | No (gitignored) | Latest raw `bcs benchmark --json` outputs |

## Record accurate baselines (preferred)

```bash
./scripts/record-benchmarks.sh
```

This:

1. Builds **release** CLI
2. Encodes small/medium/large fixtures under `benchmarks/tmp/`
3. Runs `full` + `path-hot` benchmarks (`BCS_BENCH_RUNS`, default 15)
4. Runs BCS vs JSON/YAML/TOML compares for the small profile
5. Writes:
   - `benchmarks/baseline/{small,medium,large}.json` (with `source_sha256`, `file_sha256`, host/rustc/commit, etc.)
   - `benchmarks/measured-snapshot.json`
   - `benchmarks/measured-readme-fragment.md`

Then verify:

```bash
./scripts/bench-gate.sh
```

## Fixtures

| Profile | Source | Encoded file |
|---|---|---|
| small | `examples/test.json` | `benchmarks/tmp/small.bcs` |
| medium | `examples/test-nested.json` | `benchmarks/tmp/medium.bcs` |
| large | generated `benchmarks/tmp/large.json` | `benchmarks/tmp/large.bcs` |

## Gate-only run

```bash
./scripts/bench-gate.sh
```

Compares `benchmarks/current/` against checked-in `baseline/` using thresholds in [`docs/performance-gates.md`](../docs/performance-gates.md).

## Updating published docs numbers

1. `./scripts/record-benchmarks.sh`
2. Replace the measured sections in [`docs/benchmarks.md`](../docs/benchmarks.md) using `measured-readme-fragment.md`
3. Commit baselines + `measured-snapshot.json` + `docs/benchmarks.md` together

Do **not** paste debug/`cargo run` single-run timings into docs.

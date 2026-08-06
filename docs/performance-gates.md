# Performance Gates Policy

This project is read-performance first.

Primary goal: fast and efficient runtime configuration reads.
Secondary goal: compact file size when it does not harm read latency.

## KPI Definitions

- `load_time_p95_ns`: latency to load/open BCS decoder (`Decoder::from_file`) at p95.
- `decode_time_p95_ns`: latency to decode full payload at p95.
- `path_get_p95_proxy`: current proxy metric for path lookup latency (`random_access_avg_ns` from `bcs benchmark --json`).
- `path_get_simple_p95_ns`: p95 for simple top-level path lookups.
- `path_get_deep_p95_ns`: p95 for deep nested path lookups.
- `path_get_wildcard_p95_ns`: p95 for wildcard path lookups.
- `path_get_hot_p95_ns`: p95 for repeated hot-loop path lookups.
- `file_size`: output BCS size in bytes.

## Baseline Profiles

Baseline files are stored in:

- `benchmarks/baseline/small.json`
- `benchmarks/baseline/medium.json`
- `benchmarks/baseline/large.json`

Current fixtures used by `scripts/bench-gate.sh` / `scripts/record-benchmarks.sh`:

- small: `examples/test.json`
- medium: `examples/test-nested.json`
- large: generated synthetic config in `benchmarks/tmp/large.json` (~hundreds of KB encoded)

Baselines are recorded by `./scripts/record-benchmarks.sh` and include provenance (`meta.recorded_at`, host, rustc, git, `source_sha256`, `file_sha256`). The docs-facing copy lives in `benchmarks/measured-snapshot.json`.

## Gate Thresholds

Default allowed regression vs baseline:

- Path lookup p95 proxy (`random_access_avg_ns`): max `+8%` when enough statistical signal exists.
- Path simple p95 (`path_get_simple_p95_ns`): max `+8%` when sample count >= 10 and baseline >= 10,000 ns.
- Path deep p95 (`path_get_deep_p95_ns`): max `+8%` when sample count >= 10 and baseline >= 10,000 ns.
- Path wildcard p95 (`path_get_wildcard_p95_ns`): max `+12%` when sample count >= 10 and baseline >= 10,000 ns.
- Path hot-loop p95 (`path_get_hot_p95_ns`): max `+8%` when sample count >= 10 and baseline >= 10,000 ns.
- Full decode p95 (`decode_time_p95_ns`): max `+10%`
- Load p95 (`load_time_p95_ns`): max `+10%`
- File size (`file_size`): max `+20%`

These can be overridden via environment variables in CI:

- `BCS_GATE_PATH_GET_P95_PCT` (default `8`)
- `BCS_GATE_DECODE_P95_PCT` (default `10`)
- `BCS_GATE_LOAD_P95_PCT` (default `10`)
- `BCS_GATE_SIZE_PCT` (default `20`)
- `BCS_BENCH_RUNS` (default `15`)
- `BCS_GATE_PATH_SIMPLE_P95_PCT` (default `8`)
- `BCS_GATE_PATH_DEEP_P95_PCT` (default `8`)
- `BCS_GATE_PATH_WILDCARD_P95_PCT` (default `12`)
- `BCS_GATE_PATH_HOT_P95_PCT` (default `8`)

## Running the Gate Locally

```bash
# Refresh baselines + docs snapshot from a real release measurement
./scripts/record-benchmarks.sh

# Compare current machine against checked-in baselines
./scripts/bench-gate.sh
```

The scripts print active configuration (runs and thresholds) at startup for easier CI diagnostics.

`record-benchmarks.sh` builds release CLI, prepares fixtures under `benchmarks/tmp/`, writes current results to `benchmarks/current/`, updates `benchmarks/baseline/` and `benchmarks/measured-snapshot.json`.

`bench-gate.sh` prepares fixtures under `benchmarks/tmp/`, writes current results to `benchmarks/current/`, compares with baselines, and exits non-zero on threshold violations.

The gate executes two benchmark passes:

- `full` mode (default command behavior) for global metrics and mixed path metrics.
- `path-hot` mode for repeated-path query latency (`path_get_hot_p95_ns`).

Path lookup proxy enforcement is skipped if metrics are too noisy for meaningful percent gating:

- baseline `random_access_samples < 50`, or
- current `random_access_samples < 50`, or
- baseline `random_access_avg_ns < 100`.

In those cases, the script prints a warning and does not fail on the path proxy metric.

Decode/load p95 enforcement is also skipped when baseline p95 is very small (< 50,000 ns), because percent-only deltas become unstable at that scale.

Path-specific p95 enforcement is skipped when baseline p95 is very small (< 10,000 ns), for the same reason.

## Decision Rules

- Changes that improve size but break read-latency thresholds should be rejected or made opt-in.
- Storage-level optimizations (for example tokenization/TOON-like approaches) must satisfy read-performance thresholds before becoming default behavior.
- If a regression is intentional and accepted, update baseline files in the same PR and document rationale in `CHANGELOG.md`.

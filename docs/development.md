# Development

Local build, test, lint, and release gates for BCS contributors.

## Prerequisites

- Rust 1.70+

## Build

```bash
cargo build --release
```

## Test

```bash
cargo test --workspace
```

## Lint and format

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
```

## Pre-release checklist (local)

Run this full gate before creating a release tag:

```bash
# 1) Formatting
cargo fmt --all -- --check

# 2) Lint (strict)
cargo clippy --all-targets --all-features -- -D warnings

# 3) Tests
cargo test --workspace

# 4) Release build
cargo build --release --workspace

# 5) Read-performance regression gate
./scripts/bench-gate.sh
```

Recommended install/smoke test from local source:

```bash
# Install local CLI build (or: cargo install bcs-cli after crates.io publish)
cargo install --path cli --force

# If 'bcs' is not found, ensure Cargo bin path is in PATH
export PATH="$HOME/.cargo/bin:$PATH"

# Smoke checks
mkdir -p tmp
bcs --help
bcs encode examples/test.json -o tmp/release-smoke.bcs
bcs inspect tmp/release-smoke.bcs --json
```

Automation script for the same sequence:

```bash
./scripts/pre-release-check.sh
```

Release process (tag → GitHub binaries → crates.io): see
[`.github/release-checklist.md`](../.github/release-checklist.md).
Vulnerability reporting: [SECURITY.md](../SECURITY.md).

## Performance gate (read-performance first)

```bash
./scripts/bench-gate.sh
```

To **refresh** baselines and the docs snapshot from a fresh release measurement:

```bash
./scripts/record-benchmarks.sh
# Then copy numbers from benchmarks/measured-readme-fragment.md into docs/benchmarks.md if publishing them.
./scripts/bench-gate.sh   # should pass against the new baselines
```

Optional override examples:

```bash
BCS_BENCH_RUNS=25 BCS_GATE_DECODE_P95_PCT=8 ./scripts/bench-gate.sh
BCS_BENCH_RUNS=25 BCS_GATE_PATH_HOT_P95_PCT=6 ./scripts/bench-gate.sh
```

The benchmark gate runs both `full` and `path-hot` modes. It tracks dedicated path-query metrics (`simple`, `deep`, `wildcard`, and `hot-loop` p95), in addition to decode/load/size thresholds.

Default gate thresholds:

- path lookup latency proxy (`random_access_avg_ns`): max `+8%`
- path simple p95: max `+8%`
- path deep p95: max `+8%`
- path wildcard p95: max `+12%`
- path hot-loop p95: max `+8%`
- full decode p95: max `+10%`
- load p95: max `+10%`
- file size: max `+20%`

Path metrics and decode/load metrics are enforced only when sample count and baseline latency are high enough to avoid noisy percent deltas.

Full policy and environment-variable overrides: [performance-gates.md](performance-gates.md).

## FFI / language bindings

```bash
cargo build -p bcs-ffi --release
./scripts/package-ffi.sh
./scripts/run-binding-selftests.sh
```

See [bindings.md](bindings.md).

## Related

- [Contributing Guide](../CONTRIBUTING.md)
- [Measured benchmarks](benchmarks.md)
- [Language bindings](bindings.md)

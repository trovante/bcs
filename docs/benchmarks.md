# Measured Benchmarks

Reproducible size and latency numbers from a **release** recording
(`./scripts/record-benchmarks.sh`).

Machine-readable sources:

- [`benchmarks/measured-snapshot.json`](../benchmarks/measured-snapshot.json) — full snapshot
- [`benchmarks/baseline/`](../benchmarks/baseline/) — gate baselines (same run)
- [`benchmarks/measured-readme-fragment.md`](../benchmarks/measured-readme-fragment.md) — generated fragment for docs updates

Last recorded: `2026-08-04T07:16:48Z` · `release` · `15` runs · host `Darwin 25.5.0 arm64` · `rustc 1.92.0`.

Policy and gate thresholds: [performance-gates.md](performance-gates.md).
Asset layout: [benchmarks/README.md](../benchmarks/README.md).

## Reproduce sizes

```bash
./scripts/record-benchmarks.sh
# or manually:
mkdir -p tmp
cargo build --release -p bcs-cli
./target/release/bcs encode examples/test.json -o tmp/readme.json.default.bcs
./target/release/bcs encode examples/test.json -o tmp/readme.json.compact.bcs --compact
./target/release/bcs encode examples/test.json -o tmp/readme.json.compact.compressed.bcs --compact --compress-data
# …same for test.yaml / test.toml
wc -c examples/test.json tmp/readme.json.*.bcs
```

Observed sizes (release encode):

- `test.json`: 293 bytes
  - `default`: 642 bytes
  - `compact`: 363 bytes
  - `compact + compress-data`: 363 bytes
- `test.yaml`: 209 bytes
  - `default`: 642 bytes
  - `compact`: 363 bytes
  - `compact + compress-data`: 363 bytes
- `test.toml`: 215 bytes
  - `default`: 607 bytes
  - `compact`: 363 bytes
  - `compact + compress-data`: 363 bytes

This demonstrates why BCS does not claim universal size reduction versus JSON, especially for small payloads.

## Query speed (BCS vs JSON/YAML/TOML)

Recorded with release CLI, 15 runs, p95 decode + indexed lookup average (small fixture = default encode of `examples/test.json`):

```bash
./scripts/record-benchmarks.sh
# or:
cargo build --release -p bcs-cli
./target/release/bcs encode examples/test.json -o benchmarks/tmp/small.bcs
./target/release/bcs benchmark benchmarks/tmp/small.bcs --compare examples/test.json --json --runs 15
./target/release/bcs benchmark benchmarks/tmp/small.bcs --compare examples/test.yaml --json --runs 15
./target/release/bcs benchmark benchmarks/tmp/small.bcs --compare examples/test.toml --json --runs 15
```

Observed release results (from `benchmarks/measured-snapshot.json`):

- Compared with JSON (`examples/test.json`):
  - BCS decode p95: `7.62 µs` (`7625 ns`)
  - JSON parse/decode p95: `11.79 µs` (`11792 ns`)
  - BCS indexed lookup: `41 ns` average (`4` samples)
- Compared with YAML (`examples/test.yaml`):
  - BCS decode p95: `9.71 µs` (`9709 ns`)
  - YAML parse/decode p95: `33.04 µs` (`33042 ns`)
  - BCS indexed lookup: `52 ns` average (`4` samples)
- Compared with TOML (`examples/test.toml`):
  - BCS decode p95: `6.25 µs` (`6250 ns`)
  - TOML parse/decode p95: `24.17 µs` (`24168 ns`)
  - BCS indexed lookup: `73 ns` average (`4` samples)

Gate profile summary (same recording):

| Profile | Size | Decode p95 | Path simple p95 | Path hot p95 |
|---|---:|---:|---:|---:|
| small | 642 B | 7.46 µs | 459 ns | n/a (no hot path) |
| medium | 662 B | 10.38 µs | 1.88 µs | 2.08 µs |
| large | 228510 B | 2.36 ms | 856.33 µs | 871.92 µs |

## Interpretation notes

- Text-format comparisons measure full parse/decode, not indexed path lookup.
- BCS indexed lookup uses the embedded index table (different access pattern; sample count equals indexed fields used).
- **Inspect tree (offset cursor):** `bcs inspect --tree` / `bcs dump --format debug-tree` use index entries for the root and wire walks for nested containers — they do **not** call `decode_to_value()` for the full document. Leaf previews decode only that leaf. See [adr/inspect-cursor.md](adr/inspect-cursor.md).
- Values are machine-specific. Refresh with `./scripts/record-benchmarks.sh` after intentional perf changes, then update this page from `benchmarks/measured-readme-fragment.md`.
- Prefer `--release` and multiple runs (`--runs`) for decision-making; do not cite debug single-run timings.

The benchmark command reports percentile-based latency metrics (p50 and p95) across repeated runs.

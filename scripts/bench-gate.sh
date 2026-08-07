#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

RUNS="${BCS_BENCH_RUNS:-15}"
THRESHOLD_PATH_GET="${BCS_GATE_PATH_GET_P95_PCT:-8}"
THRESHOLD_DECODE="${BCS_GATE_DECODE_P95_PCT:-10}"
THRESHOLD_LOAD="${BCS_GATE_LOAD_P95_PCT:-10}"
THRESHOLD_SIZE="${BCS_GATE_SIZE_PCT:-20}"
THRESHOLD_PATH_SIMPLE="${BCS_GATE_PATH_SIMPLE_P95_PCT:-8}"
THRESHOLD_PATH_DEEP="${BCS_GATE_PATH_DEEP_P95_PCT:-8}"
THRESHOLD_PATH_WILDCARD="${BCS_GATE_PATH_WILDCARD_P95_PCT:-12}"
THRESHOLD_PATH_HOT="${BCS_GATE_PATH_HOT_P95_PCT:-8}"
# 1 = fail on latency regressions (default, local). 0 = size-only hard fail
# (recommended on shared CI runners where p95 noise routinely exceeds 15–25%).
FAIL_ON_LATENCY="${BCS_GATE_FAIL_ON_LATENCY:-1}"

if ! [[ "$RUNS" =~ ^[0-9]+$ ]] || [ "$RUNS" -lt 1 ]; then
  echo "ERROR: BCS_BENCH_RUNS must be a positive integer (got: $RUNS)"
  exit 1
fi

if ! [[ "$FAIL_ON_LATENCY" =~ ^[01]$ ]]; then
  echo "ERROR: BCS_GATE_FAIL_ON_LATENCY must be 0 or 1 (got: $FAIL_ON_LATENCY)"
  exit 1
fi

echo "Benchmark gate config:"
echo "  runs:             $RUNS"
echo "  fail_on_latency:  $FAIL_ON_LATENCY"
echo "  path_get_p95:     +${THRESHOLD_PATH_GET}%"
echo "  decode_p95:       +${THRESHOLD_DECODE}%"
echo "  load_p95:         +${THRESHOLD_LOAD}%"
echo "  file_size:        +${THRESHOLD_SIZE}%"
echo "  path_simple_p95:  +${THRESHOLD_PATH_SIMPLE}%"
echo "  path_deep_p95:    +${THRESHOLD_PATH_DEEP}%"
echo "  path_wildcard_p95:+${THRESHOLD_PATH_WILDCARD}%"
echo "  path_hot_p95:     +${THRESHOLD_PATH_HOT}%"

if [ ! -f "benchmarks/baseline/small.json" ] || [ ! -f "benchmarks/baseline/medium.json" ] || [ ! -f "benchmarks/baseline/large.json" ]; then
  echo "ERROR: Missing baseline files under benchmarks/baseline/"
  exit 1
fi

mkdir -p benchmarks/tmp benchmarks/current

# Prefer workspace target/, then CARGO_TARGET_DIR if set.
BCS_BIN="target/release/bcs"
if [ ! -x "$BCS_BIN" ] && [ -n "${CARGO_TARGET_DIR:-}" ] && [ -x "${CARGO_TARGET_DIR}/release/bcs" ]; then
  BCS_BIN="${CARGO_TARGET_DIR}/release/bcs"
fi

echo "[1/5] Building release CLI..."
cargo build --release -p bcs-cli >/dev/null

if [ ! -x "$BCS_BIN" ]; then
  if [ -n "${CARGO_TARGET_DIR:-}" ] && [ -x "${CARGO_TARGET_DIR}/release/bcs" ]; then
    BCS_BIN="${CARGO_TARGET_DIR}/release/bcs"
  else
    echo "ERROR: Could not find release CLI binary (looked in target/release and \$CARGO_TARGET_DIR/release)"
    exit 1
  fi
fi

echo "[2/5] Preparing benchmark fixtures under benchmarks/tmp/..."
SMALL_BCS="benchmarks/tmp/small.bcs"
MEDIUM_BCS="benchmarks/tmp/medium.bcs"
LARGE_JSON="benchmarks/tmp/large.json"
LARGE_BCS="benchmarks/tmp/large.bcs"

"$BCS_BIN" encode examples/test.json -o "$SMALL_BCS" >/dev/null
"$BCS_BIN" encode examples/test-nested.json -o "$MEDIUM_BCS" >/dev/null

python3 - <<'PY'
import json
from pathlib import Path

out = Path("benchmarks/tmp/large.json")
services = []
for i in range(600):
    services.append(
        {
            "name": f"svc{i}",
            "enabled": i % 2 == 0,
            "retries": i % 5 + 1,
            "routes": [
                {"method": "GET", "paths": [f"/s{i}/health", f"/s{i}/ready"]},
                {"method": "POST", "paths": [f"/s{i}/items", f"/s{i}/items/bulk"]},
            ],
            "database": {"host": "localhost", "port": 5432 + i % 100, "name": f"app_{i}"},
            "features": ["auth", "metrics", "logs", "alerts"],
        }
    )
obj = {
    "version": "1.0",
    "env": "prod",
    "services": services,
    "thresholds": {"cpu": 0.85, "mem": 0.8, "disk": 0.9},
}
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(json.dumps(obj), encoding="utf-8")
print(f"wrote {out} ({out.stat().st_size} bytes)")
PY

"$BCS_BIN" encode "$LARGE_JSON" -o "$LARGE_BCS" >/dev/null

echo "[3/5] Running benchmark samples into benchmarks/current/..."
SMALL_CUR="benchmarks/current/bcs-bench-small.current.json"
MEDIUM_CUR="benchmarks/current/bcs-bench-medium.current.json"
LARGE_CUR="benchmarks/current/bcs-bench-large.current.json"

SMALL_HOT_CUR="benchmarks/current/bcs-bench-small.hot.current.json"
MEDIUM_HOT_CUR="benchmarks/current/bcs-bench-medium.hot.current.json"
LARGE_HOT_CUR="benchmarks/current/bcs-bench-large.hot.current.json"

"$BCS_BIN" benchmark "$SMALL_BCS" --json --runs "$RUNS" > "$SMALL_CUR"
"$BCS_BIN" benchmark "$MEDIUM_BCS" --json --runs "$RUNS" > "$MEDIUM_CUR"
"$BCS_BIN" benchmark "$LARGE_BCS" --json --runs "$RUNS" > "$LARGE_CUR"

"$BCS_BIN" benchmark "$SMALL_BCS" --mode path-hot --json --runs "$RUNS" > "$SMALL_HOT_CUR"
"$BCS_BIN" benchmark "$MEDIUM_BCS" --mode path-hot --json --runs "$RUNS" > "$MEDIUM_HOT_CUR"
"$BCS_BIN" benchmark "$LARGE_BCS" --mode path-hot --json --runs "$RUNS" > "$LARGE_HOT_CUR"

echo "[4/5] Comparing against baseline thresholds..."
python3 - "$THRESHOLD_PATH_GET" "$THRESHOLD_DECODE" "$THRESHOLD_LOAD" "$THRESHOLD_SIZE" "$THRESHOLD_PATH_SIMPLE" "$THRESHOLD_PATH_DEEP" "$THRESHOLD_PATH_WILDCARD" "$THRESHOLD_PATH_HOT" "$FAIL_ON_LATENCY" <<'PY'
import json
import sys

path_get_thr = float(sys.argv[1])
decode_thr = float(sys.argv[2])
load_thr = float(sys.argv[3])
size_thr = float(sys.argv[4])
path_simple_thr = float(sys.argv[5])
path_deep_thr = float(sys.argv[6])
path_wildcard_thr = float(sys.argv[7])
path_hot_thr = float(sys.argv[8])
fail_on_latency = sys.argv[9] == "1"

profiles = [
    (
        "small",
        "benchmarks/baseline/small.json",
        "benchmarks/current/bcs-bench-small.current.json",
        "benchmarks/current/bcs-bench-small.hot.current.json",
    ),
    (
        "medium",
        "benchmarks/baseline/medium.json",
        "benchmarks/current/bcs-bench-medium.current.json",
        "benchmarks/current/bcs-bench-medium.hot.current.json",
    ),
    (
        "large",
        "benchmarks/baseline/large.json",
        "benchmarks/current/bcs-bench-large.current.json",
        "benchmarks/current/bcs-bench-large.hot.current.json",
    ),
]

def pct_delta(curr, base):
    if base == 0:
        return 0.0
    return ((curr - base) / base) * 100.0

def should_enforce(base_value, base_samples, cur_samples, min_base, min_samples):
    return (
        base_samples >= min_samples
        and cur_samples >= min_samples
        and base_value >= min_base
    )

failures = []
for name, base_path, cur_path, hot_cur_path in profiles:
    with open(base_path, "r", encoding="utf-8") as f:
        base = json.load(f)["bcs"]
    with open(cur_path, "r", encoding="utf-8") as f:
        cur = json.load(f)["bcs"]
    with open(hot_cur_path, "r", encoding="utf-8") as f:
        hot_cur = json.load(f)["bcs"]

    d_decode = pct_delta(cur["decode_time_p95_ns"], base["decode_time_p95_ns"])
    d_load = pct_delta(cur["load_time_p95_ns"], base["load_time_p95_ns"])
    d_size = pct_delta(cur["file_size"], base["file_size"])
    d_path_simple = pct_delta(cur.get("path_get_simple_p95_ns", 0), base.get("path_get_simple_p95_ns", 0))
    d_path_deep = pct_delta(cur.get("path_get_deep_p95_ns", 0), base.get("path_get_deep_p95_ns", 0))
    d_path_wildcard = pct_delta(cur.get("path_get_wildcard_p95_ns", 0), base.get("path_get_wildcard_p95_ns", 0))
    d_path_hot = pct_delta(hot_cur.get("path_get_hot_p95_ns", 0), base.get("path_get_hot_p95_ns", 0))

    # random_access_avg_ns acts as path-get proxy in current benchmark output
    d_path = pct_delta(cur["random_access_avg_ns"], base["random_access_avg_ns"])

    print(f"{name}: decode_p95 {d_decode:+.2f}%, load_p95 {d_load:+.2f}%, path_get_p95_proxy {d_path:+.2f}%, path_simple_p95 {d_path_simple:+.2f}%, path_deep_p95 {d_path_deep:+.2f}%, path_wildcard_p95 {d_path_wildcard:+.2f}%, path_hot_p95 {d_path_hot:+.2f}%, size {d_size:+.2f}%")

    # Proxy can be noisy when sample count is very low or baseline latency is extremely small.
    # Enforce percentage threshold only when it is statistically meaningful.
    enforce_path_proxy = (
        base.get("random_access_samples", 0) >= 50
        and cur.get("random_access_samples", 0) >= 50
        and base["random_access_avg_ns"] >= 100
    )

    if enforce_path_proxy and d_path > path_get_thr:
        failures.append(f"{name}: path_get_p95_proxy regression {d_path:.2f}% > {path_get_thr:.2f}%")
    elif not enforce_path_proxy:
        print(f"{name}: path_get_p95_proxy not enforced (low sample count or tiny baseline)")
    enforce_decode = should_enforce(
        base["decode_time_p95_ns"],
        base.get("runs", 0),
        cur.get("runs", 0),
        50_000,
        10,
    )
    enforce_load = should_enforce(
        base["load_time_p95_ns"],
        base.get("runs", 0),
        cur.get("runs", 0),
        50_000,
        10,
    )

    if enforce_decode and d_decode > decode_thr:
        failures.append(f"{name}: decode_p95 regression {d_decode:.2f}% > {decode_thr:.2f}%")
    elif not enforce_decode:
        print(f"{name}: decode_p95 not enforced (low sample count or tiny baseline)")

    if enforce_load and d_load > load_thr:
        failures.append(f"{name}: load_p95 regression {d_load:.2f}% > {load_thr:.2f}%")
    elif not enforce_load:
        print(f"{name}: load_p95 not enforced (low sample count or tiny baseline)")
    if d_size > size_thr:
        failures.append(f"{name}: file_size regression {d_size:.2f}% > {size_thr:.2f}%")

    enforce_path_simple = should_enforce(
        base.get("path_get_simple_p95_ns", 0),
        base.get("path_get_simple_samples", 0),
        cur.get("path_get_simple_samples", 0),
        10_000,
        10,
    )
    enforce_path_deep = should_enforce(
        base.get("path_get_deep_p95_ns", 0),
        base.get("path_get_deep_samples", 0),
        cur.get("path_get_deep_samples", 0),
        10_000,
        10,
    )
    enforce_path_wildcard = should_enforce(
        base.get("path_get_wildcard_p95_ns", 0),
        base.get("path_get_wildcard_samples", 0),
        cur.get("path_get_wildcard_samples", 0),
        10_000,
        10,
    )
    enforce_path_hot = should_enforce(
        base.get("path_get_hot_p95_ns", 0),
        base.get("path_get_hot_samples", 0),
        hot_cur.get("path_get_hot_samples", 0),
        10_000,
        10,
    )

    if enforce_path_simple and d_path_simple > path_simple_thr:
        failures.append(f"{name}: path_simple_p95 regression {d_path_simple:.2f}% > {path_simple_thr:.2f}%")
    elif not enforce_path_simple:
        print(f"{name}: path_simple_p95 not enforced (low sample count or tiny baseline)")

    if enforce_path_deep and d_path_deep > path_deep_thr:
        failures.append(f"{name}: path_deep_p95 regression {d_path_deep:.2f}% > {path_deep_thr:.2f}%")
    elif not enforce_path_deep:
        print(f"{name}: path_deep_p95 not enforced (low sample count or tiny baseline)")

    if enforce_path_wildcard and d_path_wildcard > path_wildcard_thr:
        failures.append(f"{name}: path_wildcard_p95 regression {d_path_wildcard:.2f}% > {path_wildcard_thr:.2f}%")
    elif not enforce_path_wildcard:
        print(f"{name}: path_wildcard_p95 not enforced (low sample count or tiny baseline)")

    if enforce_path_hot and d_path_hot > path_hot_thr:
        failures.append(f"{name}: path_hot_p95 regression {d_path_hot:.2f}% > {path_hot_thr:.2f}%")
    elif not enforce_path_hot:
        print(f"{name}: path_hot_p95 not enforced (low sample count or tiny baseline)")

if failures:
    size_failures = [f for f in failures if "file_size" in f]
    latency_failures = [f for f in failures if "file_size" not in f]
    print("\nBenchmark gate regressions:")
    for f in failures:
        print(f"- {f}")
    if size_failures or (fail_on_latency and latency_failures):
        print("\nBenchmark gate FAILED")
        sys.exit(1)
    print(
        "\nBenchmark gate PASSED (latency advisory only; "
        "BCS_GATE_FAIL_ON_LATENCY=0). Size thresholds still hard-fail."
    )
    sys.exit(0)

print("\nBenchmark gate PASSED")
PY

echo "[5/5] Done"

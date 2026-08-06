#!/bin/bash
# Record release-mode benchmark baselines and a docs-facing measurement snapshot.
# Usage (from repo root):
#   ./scripts/record-benchmarks.sh
# Optional:
#   BCS_BENCH_RUNS=30 ./scripts/record-benchmarks.sh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

RUNS="${BCS_BENCH_RUNS:-15}"
if ! [[ "$RUNS" =~ ^[0-9]+$ ]] || [ "$RUNS" -lt 1 ]; then
  echo "ERROR: BCS_BENCH_RUNS must be a positive integer (got: $RUNS)"
  exit 1
fi

mkdir -p benchmarks/tmp benchmarks/current benchmarks/baseline

echo "[1/6] Building release CLI..."
cargo build --release -p bcs-cli >/dev/null

BCS_BIN="target/release/bcs"
if [ ! -x "$BCS_BIN" ] && [ -n "${CARGO_TARGET_DIR:-}" ] && [ -x "${CARGO_TARGET_DIR}/release/bcs" ]; then
  BCS_BIN="${CARGO_TARGET_DIR}/release/bcs"
fi
if [ ! -x "$BCS_BIN" ]; then
  echo "ERROR: release CLI not found"
  exit 1
fi

GIT_COMMIT="$(git rev-parse --verify HEAD 2>/dev/null || true)"
if [ -z "$GIT_COMMIT" ]; then
  GIT_COMMIT="unborn"
fi
GIT_DESCRIBE="$(git describe --always --dirty 2>/dev/null || true)"
if [ -z "$GIT_DESCRIBE" ]; then
  GIT_DESCRIBE="unborn"
fi
RUSTC_VERSION="$(rustc --version 2>/dev/null || echo unknown)"
HOST_UNAME="$(uname -srm 2>/dev/null || echo unknown)"
RECORDED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
CLI_VERSION="$("$BCS_BIN" --version 2>/dev/null | head -1 || echo unknown)"
# Strip any accidental newlines from captured metadata.
GIT_COMMIT="${GIT_COMMIT//$'\n'/}"
GIT_DESCRIBE="${GIT_DESCRIBE//$'\n'/}"
CLI_VERSION="${CLI_VERSION//$'\n'/}"

sha256_file() {
  local f="$1"
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$f" | awk '{print $1}'
  elif command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$f" | awk '{print $1}'
  else
    python3 -c "import hashlib,sys;print(hashlib.sha256(open(sys.argv[1],'rb').read()).hexdigest())" "$f"
  fi
}

echo "[2/6] Preparing fixtures under benchmarks/tmp/..."
"$BCS_BIN" encode examples/test.json -o benchmarks/tmp/small.bcs >/dev/null
"$BCS_BIN" encode examples/test-nested.json -o benchmarks/tmp/medium.bcs >/dev/null

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

"$BCS_BIN" encode benchmarks/tmp/large.json -o benchmarks/tmp/large.bcs >/dev/null

# Size matrix for docs (default / compact / compact+compress)
mkdir -p tmp
for src in examples/test.json examples/test.yaml examples/test.toml; do
  base="$(basename "$src")"
  stem="${base%.*}"
  ext="${base##*.}"
  "$BCS_BIN" encode "$src" -o "tmp/size.${stem}.${ext}.default.bcs" >/dev/null
  "$BCS_BIN" encode "$src" -o "tmp/size.${stem}.${ext}.compact.bcs" --compact >/dev/null
  "$BCS_BIN" encode "$src" -o "tmp/size.${stem}.${ext}.compact.compressed.bcs" --compact --compress-data >/dev/null
done

echo "[3/6] Running full + path-hot benchmarks..."
"$BCS_BIN" benchmark benchmarks/tmp/small.bcs --json --runs "$RUNS" > benchmarks/current/bcs-bench-small.current.json
"$BCS_BIN" benchmark benchmarks/tmp/medium.bcs --json --runs "$RUNS" > benchmarks/current/bcs-bench-medium.current.json
"$BCS_BIN" benchmark benchmarks/tmp/large.bcs --json --runs "$RUNS" > benchmarks/current/bcs-bench-large.current.json

"$BCS_BIN" benchmark benchmarks/tmp/small.bcs --mode path-hot --json --runs "$RUNS" > benchmarks/current/bcs-bench-small.hot.current.json
"$BCS_BIN" benchmark benchmarks/tmp/medium.bcs --mode path-hot --json --runs "$RUNS" > benchmarks/current/bcs-bench-medium.hot.current.json
"$BCS_BIN" benchmark benchmarks/tmp/large.bcs --mode path-hot --json --runs "$RUNS" > benchmarks/current/bcs-bench-large.hot.current.json

echo "[4/6] Running compare (BCS vs JSON/YAML/TOML) for docs snapshot..."
"$BCS_BIN" benchmark benchmarks/tmp/small.bcs --compare examples/test.json --json --runs "$RUNS" \
  > benchmarks/current/bcs-bench-small.compare.json.json
"$BCS_BIN" benchmark benchmarks/tmp/small.bcs --compare examples/test.yaml --json --runs "$RUNS" \
  > benchmarks/current/bcs-bench-small.compare.yaml.json
"$BCS_BIN" benchmark benchmarks/tmp/small.bcs --compare examples/test.toml --json --runs "$RUNS" \
  > benchmarks/current/bcs-bench-small.compare.toml.json

echo "[5/6] Writing baselines + docs snapshot..."
export RECORDED_AT GIT_COMMIT GIT_DESCRIBE RUSTC_VERSION HOST_UNAME CLI_VERSION RUNS
python3 - <<'PY'
import json
import os
from pathlib import Path

def sha256(path: Path) -> str:
    import hashlib
    return hashlib.sha256(path.read_bytes()).hexdigest()

def file_size(path: Path) -> int:
    return path.stat().st_size

meta = {
    "recorded_at": os.environ["RECORDED_AT"],
    "git_commit": os.environ["GIT_COMMIT"],
    "git_describe": os.environ["GIT_DESCRIBE"],
    "rustc": os.environ["RUSTC_VERSION"],
    "host": os.environ["HOST_UNAME"],
    "cli": os.environ["CLI_VERSION"],
    "build_profile": "release",
    "runs": int(os.environ["RUNS"]),
}

profiles = [
    ("small", "examples/test.json", "benchmarks/tmp/small.bcs",
     "benchmarks/current/bcs-bench-small.current.json",
     "benchmarks/current/bcs-bench-small.hot.current.json"),
    ("medium", "examples/test-nested.json", "benchmarks/tmp/medium.bcs",
     "benchmarks/current/bcs-bench-medium.current.json",
     "benchmarks/current/bcs-bench-medium.hot.current.json"),
    ("large", "benchmarks/tmp/large.json", "benchmarks/tmp/large.bcs",
     "benchmarks/current/bcs-bench-large.current.json",
     "benchmarks/current/bcs-bench-large.hot.current.json"),
]

baselines = {}
for name, source, encoded, full_path, hot_path in profiles:
    full = json.loads(Path(full_path).read_text(encoding="utf-8"))
    hot = json.loads(Path(hot_path).read_text(encoding="utf-8"))
    bcs = dict(full["bcs"])
    # Merge hot-loop metrics from dedicated path-hot pass (more samples / focused).
    bcs["path_get_hot_p95_ns"] = hot["bcs"].get("path_get_hot_p95_ns", 0)
    bcs["path_get_hot_samples"] = hot["bcs"].get("path_get_hot_samples", 0)

    source_path = Path(source)
    encoded_path = Path(encoded)
    payload = {
        "profile": name,
        "source": source,
        "source_sha256": sha256(source_path),
        "source_bytes": file_size(source_path),
        "file": encoded,
        "file_sha256": sha256(encoded_path),
        "file_bytes": file_size(encoded_path),
        "runs": meta["runs"],
        "meta": meta,
        "bcs": bcs,
        "path_hot": {
            "path_get_hot_p95_ns": hot["bcs"].get("path_get_hot_p95_ns", 0),
            "path_get_hot_samples": hot["bcs"].get("path_get_hot_samples", 0),
            "runs": hot.get("runs", meta["runs"]),
        },
    }
    if name == "large":
        payload["note"] = "large.json is generated by scripts/record-benchmarks.sh / bench-gate.sh"
    out = Path(f"benchmarks/baseline/{name}.json")
    out.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    baselines[name] = payload
    print(f"wrote {out}")

# Size matrix
size_rows = []
for stem, src in [("test", "examples/test.json"), ("test", "examples/test.yaml"), ("test", "examples/test.toml")]:
    src_path = Path(src)
    ext = src_path.suffix.lstrip(".")
    row = {
        "source": src,
        "source_bytes": file_size(src_path),
        "default_bcs_bytes": file_size(Path(f"tmp/size.{stem}.{ext}.default.bcs")),
        "compact_bcs_bytes": file_size(Path(f"tmp/size.{stem}.{ext}.compact.bcs")),
        "compact_compressed_bcs_bytes": file_size(Path(f"tmp/size.{stem}.{ext}.compact.compressed.bcs")),
    }
    size_rows.append(row)

compares = {}
for fmt in ("json", "yaml", "toml"):
    path = Path(f"benchmarks/current/bcs-bench-small.compare.{fmt}.json")
    data = json.loads(path.read_text(encoding="utf-8"))
    compares[fmt] = {
        "compare_file": data["compare"]["file"],
        "bcs": {
            "decode_time_p50_ns": data["bcs"]["decode_time_p50_ns"],
            "decode_time_p95_ns": data["bcs"]["decode_time_p95_ns"],
            "random_access_avg_ns": data["bcs"]["random_access_avg_ns"],
            "random_access_samples": data["bcs"]["random_access_samples"],
            "file_size": data["bcs"]["file_size"],
        },
        "text": {
            "decode_time_p50_ns": data["compare"]["results"]["decode_time_p50_ns"],
            "decode_time_p95_ns": data["compare"]["results"]["decode_time_p95_ns"],
            "file_size": data["compare"]["results"]["file_size"],
        },
        "comparison": data["compare"]["comparison"],
    }

snapshot = {
    "meta": meta,
    "sizes": size_rows,
    "baselines": {
        name: {
            "source": p["source"],
            "file_bytes": p["file_bytes"],
            "decode_time_p95_ns": p["bcs"]["decode_time_p95_ns"],
            "load_time_p95_ns": p["bcs"]["load_time_p95_ns"],
            "path_get_simple_p95_ns": p["bcs"]["path_get_simple_p95_ns"],
            "path_get_deep_p95_ns": p["bcs"]["path_get_deep_p95_ns"],
            "path_get_wildcard_p95_ns": p["bcs"]["path_get_wildcard_p95_ns"],
            "path_get_hot_p95_ns": p["bcs"]["path_get_hot_p95_ns"],
            "random_access_avg_ns": p["bcs"]["random_access_avg_ns"],
            "random_access_samples": p["bcs"]["random_access_samples"],
        }
        for name, p in baselines.items()
    },
    "compare_small": compares,
}

snap_path = Path("benchmarks/measured-snapshot.json")
snap_path.write_text(json.dumps(snapshot, indent=2) + "\n", encoding="utf-8")
print(f"wrote {snap_path}")

# Human-readable markdown fragment for docs/benchmarks.md copy/paste
def fmt_ns(ns: int) -> str:
    if ns >= 1_000_000:
        return f"{ns / 1_000_000:.2f} ms"
    if ns >= 1_000:
        return f"{ns / 1_000:.2f} µs"
    return f"{ns} ns"

lines = []
lines.append(f"<!-- generated by scripts/record-benchmarks.sh at {meta['recorded_at']} -->")
lines.append(f"<!-- build={meta['build_profile']} runs={meta['runs']} commit={meta['git_describe']} host={meta['host']} -->")
lines.append("")
lines.append("Observed sizes (release encode, see `benchmarks/measured-snapshot.json`):")
lines.append("")
for row in size_rows:
    src = Path(row["source"]).name
    lines.append(f"- `{src}`: {row['source_bytes']} bytes")
    lines.append(f"  - `default`: {row['default_bcs_bytes']} bytes")
    lines.append(f"  - `compact`: {row['compact_bcs_bytes']} bytes")
    lines.append(f"  - `compact + compress-data`: {row['compact_compressed_bcs_bytes']} bytes")
lines.append("")
lines.append("Observed release latency (small profile, p95 / indexed lookup avg):")
lines.append("")
small = baselines["small"]["bcs"]
lines.append(f"- BCS decode p95: `{fmt_ns(small['decode_time_p95_ns'])}` (`{small['decode_time_p95_ns']} ns`)")
lines.append(f"- BCS load p95: `{fmt_ns(small['load_time_p95_ns'])}` (`{small['load_time_p95_ns']} ns`)")
lines.append(
    f"- BCS indexed lookup avg: `{fmt_ns(small['random_access_avg_ns'])}` "
    f"(`{small['random_access_samples']}` samples)"
)
for fmt, c in compares.items():
    lines.append(f"- vs {fmt.upper()} (`{c['compare_file']}`):")
    lines.append(f"  - BCS decode p95: `{fmt_ns(c['bcs']['decode_time_p95_ns'])}`")
    lines.append(f"  - Text parse/decode p95: `{fmt_ns(c['text']['decode_time_p95_ns'])}`")
    lines.append(
        f"  - BCS indexed lookup avg: `{fmt_ns(c['bcs']['random_access_avg_ns'])}` "
        f"(`{c['bcs']['random_access_samples']}` samples)"
    )
lines.append("")
lines.append("Gate profiles (release):")
lines.append("")
for name, p in baselines.items():
    b = p["bcs"]
    lines.append(
        f"- `{name}`: size={p['file_bytes']} B, decode_p95={fmt_ns(b['decode_time_p95_ns'])}, "
        f"path_simple_p95={fmt_ns(b['path_get_simple_p95_ns'])}, "
        f"path_hot_p95={fmt_ns(b['path_get_hot_p95_ns'])}"
    )

frag = Path("benchmarks/measured-readme-fragment.md")
frag.write_text("\n".join(lines) + "\n", encoding="utf-8")
print(f"wrote {frag}")
PY

echo "[6/6] Done."
echo "Baselines refreshed under benchmarks/baseline/"
echo "Docs snapshot: benchmarks/measured-snapshot.json"
echo "Docs fragment: benchmarks/measured-readme-fragment.md (paste into docs/benchmarks.md)"
echo "Re-run ./scripts/bench-gate.sh to verify the new baselines pass."

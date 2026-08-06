#!/bin/bash
set -euo pipefail

echo "[1/7] Checking formatting..."
cargo fmt --all -- --check

echo "[2/7] Running clippy (warnings denied)..."
cargo clippy --all-targets --all-features -- -D warnings

echo "[3/7] Running workspace tests..."
cargo test --workspace

echo "[4/7] Building release artifacts..."
cargo build --release --workspace

echo "[5/7] Running benchmark regression gate..."
./scripts/bench-gate.sh

echo "[6/7] Installing local CLI for smoke test..."
cargo install --path cli --force

BCS_BIN="${HOME}/.cargo/bin/bcs"
if [ ! -x "$BCS_BIN" ]; then
  echo "ERROR: Expected installed CLI at $BCS_BIN"
  exit 1
fi

echo "[7/7] Running smoke checks..."
mkdir -p tmp
"$BCS_BIN" --help >/dev/null
"$BCS_BIN" encode examples/test.json -o tmp/release-smoke.bcs >/dev/null
"$BCS_BIN" inspect tmp/release-smoke.bcs --json >/dev/null

echo "✓ Pre-release local checks passed"

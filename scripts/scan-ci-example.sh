#!/usr/bin/env bash
# Example CI / pre-commit hook for `bcs scan`.
# Usage: ./scripts/scan-ci-example.sh [path]
# Exit 1 on findings (default --fail-on finding).

set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="${1:-$ROOT/examples}"

if command -v bcs >/dev/null 2>&1; then
  BCS=(bcs)
elif [[ -x "$ROOT/target/release/bcs" ]]; then
  BCS=("$ROOT/target/release/bcs")
elif [[ -x "$ROOT/target/debug/bcs" ]]; then
  BCS=("$ROOT/target/debug/bcs")
else
  BCS=(cargo run -q -p bcs-cli --)
fi

echo "Scanning: $TARGET"
"${BCS[@]}" scan "$TARGET" --json --fail-on finding

#!/usr/bin/env bash
# Run language-binding self-tests that have tooling available locally.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

cargo build -p bcs-ffi --release
./scripts/package-ffi.sh >/dev/null

echo "== Python =="
PYTHONPATH=bindings/python python3 -m bcs

echo "== TypeScript =="
(
  cd bindings/typescript
  npm install --silent
  npm run selftest
)

echo "== Swift =="
(
  cd bindings/swift
  swift run BcsSelfTest
)

if command -v dotnet >/dev/null 2>&1; then
  echo "== C# =="
  (
    cd bindings/csharp
    dotnet run --project Bcs.SelfTest
  )
else
  echo "== C# == (skipped: dotnet SDK not found)"
fi

if command -v javac >/dev/null 2>&1 && javac -version >/dev/null 2>&1; then
  echo "== Java =="
  ./bindings/java/run-selftest.sh
else
  echo "== Java == (skipped: working JDK 22+ not found)"
fi

echo "Binding self-tests finished."

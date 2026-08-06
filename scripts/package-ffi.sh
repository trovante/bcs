#!/usr/bin/env bash
# Package bcs-ffi shared library + header for language bindings.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

cargo build -p bcs-ffi --release

TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
RELEASE_DIR="$TARGET_DIR/release"

OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"
case "$ARCH" in
  x86_64|amd64) ARCH="x64" ;;
  aarch64|arm64) ARCH="arm64" ;;
esac

OUT="$ROOT/dist/ffi/${OS}-${ARCH}"
mkdir -p "$OUT"

cp "$ROOT/ffi/include/bcs.h" "$OUT/bcs.h"

case "$OS" in
  darwin)
    cp "$RELEASE_DIR/libbcs_ffi.dylib" "$OUT/"
    ;;
  linux)
    cp "$RELEASE_DIR/libbcs_ffi.so" "$OUT/"
    ;;
  mingw*|msys*|cygwin*|windows*)
    cp "$RELEASE_DIR/bcs_ffi.dll" "$OUT/"
    cp "$RELEASE_DIR/bcs_ffi.lib" "$OUT/" 2>/dev/null || true
    ;;
  *)
    echo "Unsupported OS: $OS" >&2
    exit 1
    ;;
esac

cp "$ROOT/ffi/README.md" "$OUT/README.md"
echo "Packaged FFI natives at $OUT"
ls -la "$OUT"

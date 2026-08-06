#!/usr/bin/env bash
# Compile and run the Java FFM self-test (requires JDK 22+).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

if ! command -v javac >/dev/null 2>&1 || ! javac -version >/dev/null 2>&1; then
  echo "Working JDK not found. Install JDK 22+ to run Java bindings." >&2
  exit 1
fi

JAVA_VER="$(javac -version 2>&1 | awk '{print $2}' | cut -d. -f1)"
if [[ -z "${JAVA_VER}" || "${JAVA_VER}" -lt 22 ]]; then
  echo "Java bindings require JDK 22+ (found '${JAVA_VER:-unknown}')." >&2
  exit 1
fi

cargo build -p bcs-ffi --release
./scripts/package-ffi.sh >/dev/null

SRC="bindings/java/src/main/java/com/trovante/bcs/Bcs.java"
OUT="bindings/java/out"
mkdir -p "$OUT"
javac --enable-preview -source 22 -d "$OUT" "$SRC"
java --enable-native-access=ALL-UNNAMED -cp "$OUT" com.trovante.bcs.Bcs

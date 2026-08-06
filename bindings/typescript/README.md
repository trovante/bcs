# BCS TypeScript / Node bindings

Thin Node wrapper around `libbcs_ffi` using [koffi](https://koffi.dev/).

> Full guide (generate natives + all languages): **[docs/bindings.md](../../docs/bindings.md)**

## Prerequisites

```bash
# From repo root
cargo build -p bcs-ffi --release
./scripts/package-ffi.sh   # optional
```

Or:

```bash
export BCS_FFI_LIB=/absolute/path/to/libbcs_ffi.dylib
```

## Install & test

```bash
cd bindings/typescript
npm install
npm run selftest
```

## Usage

```ts
import {
  encodeJson,
  decodeToJson,
  getPathJson,
  validate,
  protectJson,
} from "@trovante/bcs";

const data = encodeJson(JSON.stringify({ server: { host: "localhost" } }));
console.log(validate(data));
console.log(getPathJson(data, "server.host")); // "localhost"
```

## Notes

- Requires Node 18+.
- Native library must match OS/arch (`darwin-arm64`, `linux-x64`, …).
- Future npm releases should ship prebuilt natives under `vendor/`.

# BCS Swift bindings

Swift package that `dlopen`s `libbcs_ffi` and exposes an idiomatic API.

> Full guide (generate natives + all languages): **[docs/bindings.md](../../docs/bindings.md)**

## Prerequisites

```bash
cargo build -p bcs-ffi --release
./scripts/package-ffi.sh
```

Optional:

```bash
export BCS_FFI_LIB=/absolute/path/to/libbcs_ffi.dylib
```

## Self-test

```bash
cd bindings/swift
swift run BcsSelfTest
```

## Usage

```swift
import Bcs

let data = try Bcs.encodeJson(#"{"server":{"host":"localhost"}}"#)
print(try Bcs.validate(data))
print(try Bcs.getPathJson(data, path: "server.host"))
```

## Notes

- macOS 13+ / Swift 5.9+
- Library discovery mirrors Python/TS (dist/ffi, target/*, `BCS_FFI_LIB`, `CARGO_TARGET_DIR`)
- Linux can use the same sources once `libbcs_ffi.so` is available

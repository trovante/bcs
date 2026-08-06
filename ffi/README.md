# BCS FFI (C ABI)

Stable-ish C ABI for using Binary Config Schema from other languages.

> End-to-end guide (build + package + language examples): **[docs/bindings.md](../docs/bindings.md)**

## How to generate the native library

From the repository root:

```bash
cargo build -p bcs-ffi --release
```

Artifacts (respects `CARGO_TARGET_DIR` if set):

| Platform | Shared | Static |
|---|---|---|
| macOS | `…/release/libbcs_ffi.dylib` | `…/release/libbcs_ffi.a` |
| Linux | `…/release/libbcs_ffi.so` | `…/release/libbcs_ffi.a` |
| Windows | `…/release/bcs_ffi.dll` | `…/release/bcs_ffi.lib` |

Public header: [`include/bcs.h`](include/bcs.h)

## How to package for distribution

```bash
./scripts/package-ffi.sh
```

Creates a portable folder:

```text
dist/ffi/<os>-<arch>/
  bcs.h
  libbcs_ffi.dylib|so|dll
  README.md
```

Example on Apple Silicon:

```text
dist/ffi/darwin-arm64/libbcs_ffi.dylib
```

Ship that folder (or the matching OS/arch build) with your language package.

## How to use from C/C++

```c
#include "bcs.h"

uint8_t *out = NULL;
size_t out_len = 0;
int rc = bcs_encode_json("{\"a\":1}", 0, 0, &out, &out_len);
if (rc != BCS_OK) {
    fprintf(stderr, "%s\n", bcs_last_error());
    return 1;
}
/* use out[0..out_len) */
bcs_free_buffer(out, out_len);
```

Compile/link (macOS example):

```bash
cc app.c \
  -I dist/ffi/darwin-arm64 \
  -L dist/ffi/darwin-arm64 \
  -lbcs_ffi \
  -o app
```

## Language wrappers in this repo

| Language | Location | Notes |
|---|---|---|
| Python | [`bindings/python`](../bindings/python) | `ctypes` |
| TypeScript / Node | [`bindings/typescript`](../bindings/typescript) | `koffi` |
| Swift | [`bindings/swift`](../bindings/swift) | `dlopen` |
| C# | [`bindings/csharp`](../bindings/csharp) | .NET 8 |
| Java | [`bindings/java`](../bindings/java) | JDK 22+ FFM |
| C / C++ | this crate + `bcs.h` | direct link |

## API surface

| Function | Purpose |
|---|---|
| `bcs_encode_json` | JSON → BCS |
| `bcs_decode_to_json` | BCS → JSON (optional password; secret refs masked; protected masked) |
| `bcs_decode_to_json_ex` | BCS → JSON with secret-resolve + optional KMS unwrap callbacks |
| `bcs_strdup` / `bcs_alloc` | Allocate C string / bytes for callback return values |
| `bcs_get_path_json` | Path query → JSON |
| `bcs_validate` | Integrity/decodability check |
| `bcs_protect_json` | Password (`pbkdf2`) encrypt sensitive paths → BCS |
| `bcs_protect_json_ex` | Password or KMS wrap (`kms`) protect → BCS |
| `bcs_last_error` / `bcs_version` | Diagnostics |
| `bcs_free_buffer` / `bcs_free_string` | Ownership cleanup |

Error codes are defined in `bcs.h` (`BCS_OK`, `BCS_ERR_*`).

## Ownership rules

1. Any `out_ptr` / `out_json` returned by BCS must be freed with the matching `bcs_free_*`.
2. Do not free the pointer from `bcs_last_error` or `bcs_version`.
3. `bcs_last_error` is **thread-local** and overwritten on the next failing call.

## Related

- [Language Bindings Guide](../docs/bindings.md)
- [Bindings status matrix](../bindings/README.md)
- [`scripts/package-ffi.sh`](../scripts/package-ffi.sh)
- [`scripts/run-binding-selftests.sh`](../scripts/run-binding-selftests.sh)

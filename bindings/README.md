# Language bindings

BCS ships a **C ABI** (`ffi/`) that other languages call into.

> Full how-to (generate natives + use every language): **[docs/bindings.md](../docs/bindings.md)**

| Language | Status | Entry point | Self-test |
|---|---|---|---|
| C / C++ | Ready | `ffi/include/bcs.h` + `libbcs_ffi` | link + call |
| Python | Ready (alpha) | [`python/`](python/) | `PYTHONPATH=bindings/python python3 -m bcs` |
| TypeScript / Node | Ready (alpha) | [`typescript/`](typescript/) | `npm run selftest` |
| Swift | Ready (alpha) | [`swift/`](swift/) | `swift run BcsSelfTest` |
| C# (.NET 8) | Ready (alpha) | [`csharp/`](csharp/) | `dotnet run --project Bcs.SelfTest` |
| Java (JDK 22+) | Ready (alpha) | [`java/`](java/) | `./bindings/java/run-selftest.sh` |

## Generate the native library

```bash
# From repository root
cargo build -p bcs-ffi --release
./scripts/package-ffi.sh
```

This creates:

```text
dist/ffi/<os>-<arch>/bcs.h
dist/ffi/<os>-<arch>/libbcs_ffi.*
```

Optional override for wrappers:

```bash
export BCS_FFI_LIB=/absolute/path/to/libbcs_ffi.dylib
```

## Run all available self-tests

```bash
./scripts/run-binding-selftests.sh
```

## Per-language docs

| Language | README |
|---|---|
| Python | [python/README.md](python/README.md) |
| TypeScript | [typescript/README.md](typescript/README.md) |
| Swift | [swift/README.md](swift/README.md) |
| C# | [csharp/README.md](csharp/README.md) |
| Java | [java/README.md](java/README.md) |
| C ABI | [../ffi/README.md](../ffi/README.md) |

## Adding another language

1. Build/package natives (`./scripts/package-ffi.sh`)
2. Load `libbcs_ffi` / `bcs_ffi.dll`
3. Bind the functions from [`bcs.h`](../ffi/include/bcs.h)
4. Wrap ownership (`bcs_free_buffer` / `bcs_free_string`)
5. Surface idiomatic APIs (`encodeJson`, `decodeToJson`, …)

Keep wrappers thin: no reimplementation of the BCS format.

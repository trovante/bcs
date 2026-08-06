# Language Bindings Guide

This guide explains how to **build** the native BCS library and **use** it from other languages.

BCS is implemented in Rust. Other languages do not reimplement the format: they call a shared C ABI (`libbcs_ffi`) through thin wrappers.

```text
┌─────────────┐   ┌──────────────┐   ┌─────────────┐
│  Python     │   │ TypeScript   │   │ Swift/C#/…  │
│  ctypes     │   │ koffi        │   │ P/Invoke…   │
└──────┬──────┘   └──────┬───────┘   └──────┬──────┘
       │                 │                  │
       └─────────────────┼──────────────────┘
                         ▼
              ┌─────────────────────┐
              │  libbcs_ffi (C ABI) │  ← ffi/
              │  + bcs.h            │
              └──────────┬──────────┘
                         ▼
              ┌─────────────────────┐
              │     bcs-core        │  ← core/
              └─────────────────────┘
```

## Quick start (generate natives)

From the repository root:

```bash
# 1) Build the shared library
cargo build -p bcs-ffi --release

# 2) Copy library + header into a portable folder
./scripts/package-ffi.sh

# 3) Optional: smoke-test every binding your machine can run
./scripts/run-binding-selftests.sh
```

All first-party bindings expose **agent-safe schema export** (`bcs_schema_export_json` / `SchemaExportJson` / `schemaExportJson` / `schema_export_json`) — paths and sensitivity flags without data-layer values. See [agent-schema.md](agent-schema.md).

### Trust boundary: FFI vs MCP

| Surface | Unlock (password / KMS / resolve) | Policy |
|---------|-----------------------------------|--------|
| **MCP** (`bcs-mcp`) | None — tools never accept credentials | Agent-safe by construction ([mcp.md](mcp.md)) |
| **FFI / language bindings** | Optional — host may pass password or resolve/unwrap callbacks | **Host-trusted**: same capability as CLI unlock; the embedding app is responsible for not logging or forwarding secrets |

Treat FFI decode-with-password as an operator/host API, not an agent-safe channel. Prefer `bcs_schema_export_json` and always-masked `bcs_get_path_json` when calling from agent tooling. See [security-review.md](security-review.md).

Packaged output:

```text
dist/ffi/<os>-<arch>/
  bcs.h
  libbcs_ffi.dylib   # macOS
  # or libbcs_ffi.so # Linux
  # or bcs_ffi.dll   # Windows
  README.md
```

Examples:

- macOS Apple Silicon → `dist/ffi/darwin-arm64/`
- Linux x86_64 → `dist/ffi/linux-x64/`

If your build uses a custom Cargo target directory, set it before packaging:

```bash
export CARGO_TARGET_DIR=/path/to/target
./scripts/package-ffi.sh
```

To force wrappers to load a specific binary:

```bash
export BCS_FFI_LIB=/absolute/path/to/libbcs_ffi.dylib
```

## What the C ABI exposes

Header: [`ffi/include/bcs.h`](../ffi/include/bcs.h)

| Function | Purpose |
|---|---|
| `bcs_encode_json` | JSON text → BCS bytes |
| `bcs_decode_to_json` | BCS bytes → JSON text (optional password; secret refs masked; protected masked) |
| `bcs_decode_to_json_ex` | Same + optional secret-resolve and KMS unwrap callbacks |
| `bcs_strdup` | Allocate string for resolve-callback returns |
| `bcs_get_path_json` | Path query → JSON value |
| `bcs_validate` | Check that a BCS buffer is decodable |
| `bcs_protect_json` | Password (`pbkdf2`) encrypt sensitive paths → BCS |
| `bcs_protect_json_ex` | Password or KMS (`kms`) protect via host wrap callback → BCS |
| `bcs_last_error` | Thread-local error message |
| `bcs_version` | Library version string |
| `bcs_free_buffer` | Free bytes returned by encode/protect |
| `bcs_free_string` | Free strings returned by decode/path |

Status codes: `BCS_OK`, `BCS_ERR_NULL`, `BCS_ERR_UTF8`, `BCS_ERR_FORMAT`, `BCS_ERR_INVALID_ARG`, `BCS_ERR_INTERNAL`.

### Ownership rules

1. Free every `out_ptr` / `out_json` with the matching `bcs_free_*`.
2. Never free `bcs_last_error()` or `bcs_version()` pointers.
3. `bcs_last_error()` is thread-local and overwritten on the next failing call.

### Direct C usage sketch

```c
#include "bcs.h"
#include <stdio.h>
#include <stdlib.h>

int main(void) {
    uint8_t *out = NULL;
    size_t out_len = 0;
    int rc = bcs_encode_json("{\"server\":{\"host\":\"localhost\"}}", 0, 0, &out, &out_len);
    if (rc != BCS_OK) {
        fprintf(stderr, "%s\n", bcs_last_error());
        return 1;
    }

    char *json = NULL;
    rc = bcs_get_path_json(out, out_len, "server.host", &json);
    if (rc == BCS_OK) {
        puts(json);              /* "localhost" */
        bcs_free_string(json);
    }

    bcs_free_buffer(out, out_len);
    return 0;
}
```

Link example (macOS):

```bash
cc example.c -I dist/ffi/darwin-arm64 -L dist/ffi/darwin-arm64 -lbcs_ffi -o example
```

---

## Python

Location: [`bindings/python`](../bindings/python)

### Generate / prepare

```bash
cargo build -p bcs-ffi --release
./scripts/package-ffi.sh
```

### Install (editable)

```bash
cd bindings/python
pip install -e .
```

Or run without installing:

```bash
PYTHONPATH=bindings/python python3 -m bcs
```

### Use

```python
import os
from bcs import encode_json, decode_to_json, get_path_json, validate, protect_json

data = encode_json('{"server":{"host":"localhost"}}')
assert validate(data)
print(get_path_json(data, "server.host"))  # "localhost"

protected = protect_json(
    '{"database":{"password":"secret"}}',
    ["database.password"],
    "master",
)
print(decode_to_json(protected))                   # masked
print(decode_to_json(protected, password="master"))  # revealed

# KMS scheme via wrap/unwrap callables (host KeyWrapper)
def xor_wrap(_p, _k, dek): return bytes(b ^ 0xA5 for b in dek)
def xor_unwrap(_p, _k, wrapped): return bytes(b ^ 0xA5 for b in wrapped)
kms = protect_json('{"t":"x"}', ["t"], kms_provider="cmd", kms_key="k1", wrap_key=xor_wrap)
print(decode_to_json(kms, unwrap_key=xor_unwrap))

# Resolve secret refs via a Python callable (scheme, locator) -> str | None
ref = encode_json('{"t":"__bcs_secret_ref__:env:API_TOKEN"}')
print(decode_to_json(ref))  # [SECRET_REF]
print(decode_to_json(ref, resolve_secrets=lambda s, n: os.environ.get(n) if s == "env" else None))
```

More detail: [`bindings/python/README.md`](../bindings/python/README.md)

---

## TypeScript / Node

Location: [`bindings/typescript`](../bindings/typescript)

Requires Node 18+.

### Generate / prepare

```bash
cargo build -p bcs-ffi --release
./scripts/package-ffi.sh
cd bindings/typescript
npm install
npm run selftest
```

### Use

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

const protectedBytes = protectJson(
  JSON.stringify({ database: { password: "secret" } }),
  ["database.password"],
  "master"
);
console.log(decodeToJson(protectedBytes));
console.log(decodeToJson(protectedBytes, "master"));
```

More detail: [`bindings/typescript/README.md`](../bindings/typescript/README.md)

---

## Swift

Location: [`bindings/swift`](../bindings/swift)

Requires Swift 5.9+ / macOS 13+.

### Generate / prepare

```bash
cargo build -p bcs-ffi --release
./scripts/package-ffi.sh
cd bindings/swift
swift run BcsSelfTest
```

### Use

```swift
import Bcs

let data = try Bcs.encodeJson(#"{"server":{"host":"localhost"}}"#)
print(try Bcs.validate(data))
print(try Bcs.getPathJson(data, path: "server.host"))

let protected = try Bcs.protectJson(
    #"{"database":{"password":"secret"}}"#,
    paths: ["database.password"],
    password: "master"
)
print(try Bcs.decodeToJson(protected))
print(try Bcs.decodeToJson(protected, password: "master"))
```

More detail: [`bindings/swift/README.md`](../bindings/swift/README.md)

---

## C# (.NET 8)

Location: [`bindings/csharp`](../bindings/csharp)

Requires .NET 8 SDK.

### Generate / prepare

```bash
cargo build -p bcs-ffi --release
./scripts/package-ffi.sh
cd bindings/csharp
dotnet run --project Bcs.SelfTest
```

### Use

```csharp
using Bcs;

var data = BcsClient.EncodeJson("""{"server":{"host":"localhost"}}""");
Console.WriteLine(BcsClient.Validate(data));
Console.WriteLine(BcsClient.GetPathJson(data, "server.host"));

var protectedBytes = BcsClient.ProtectJson(
    """{"database":{"password":"secret"}}""",
    ["database.password"],
    "master");
Console.WriteLine(BcsClient.DecodeToJson(protectedBytes));
Console.WriteLine(BcsClient.DecodeToJson(protectedBytes, "master"));
```

Reference the library project from your app:

```xml
<ItemGroup>
  <ProjectReference Include="path/to/bindings/csharp/Bcs/Bcs.csproj" />
</ItemGroup>
```

More detail: [`bindings/csharp/README.md`](../bindings/csharp/README.md)

---

## Java (JDK 22+)

Location: [`bindings/java`](../bindings/java)

Uses the Foreign Function & Memory API (Panama). No JNI stubs.

### Generate / prepare

```bash
cargo build -p bcs-ffi --release
./scripts/package-ffi.sh
./bindings/java/run-selftest.sh
```

### Use

```java
import com.trovante.bcs.Bcs;
import java.util.List;

byte[] data = Bcs.encodeJson("{\"server\":{\"host\":\"localhost\"}}");
System.out.println(Bcs.validate(data));
System.out.println(Bcs.getPathJson(data, "server.host"));

byte[] protectedBytes = Bcs.protectJson(
    "{\"database\":{\"password\":\"secret\"}}",
    List.of("database.password"),
    "master");
System.out.println(Bcs.decodeToJson(protectedBytes));
System.out.println(Bcs.decodeToJson(protectedBytes, "master"));
```

Compile/run (manual):

```bash
javac -d bindings/java/out \
  bindings/java/src/main/java/com/trovante/bcs/Bcs.java
java --enable-native-access=ALL-UNNAMED -cp bindings/java/out com.trovante.bcs.Bcs
```

More detail: [`bindings/java/README.md`](../bindings/java/README.md)

---

## Library discovery order

All wrappers look for the native library in this order:

1. `BCS_FFI_LIB` (exact file path)
2. `dist/ffi/<os>-<arch>/`
3. `target/release/` then `target/debug/`
4. `$CARGO_TARGET_DIR/release` and `$CARGO_TARGET_DIR/debug` (if set)

This is why running `./scripts/package-ffi.sh` after a release build is the most reliable local workflow.

## Scripts reference

| Script | Purpose |
|---|---|
| [`scripts/package-ffi.sh`](../scripts/package-ffi.sh) | Build release FFI (if needed) and copy natives + `bcs.h` into `dist/ffi/<os>-<arch>/` |
| [`scripts/run-binding-selftests.sh`](../scripts/run-binding-selftests.sh) | Package natives, then run Python/TS/Swift self-tests; C#/Java when SDKs exist |

## Troubleshooting

| Symptom | Fix |
|---|---|
| `Could not load bcs_ffi` / missing dylib/so/dll | Run `cargo build -p bcs-ffi --release` and `./scripts/package-ffi.sh`, or set `BCS_FFI_LIB` |
| Wrong architecture | Match `dist/ffi/<os>-<arch>` to your CPU (`arm64` vs `x64`) |
| Java stub on macOS (`Unable to locate a Java Runtime`) | Install a real JDK 22+, not only the `/usr/bin/java` stub |
| C# skipped | Install .NET 8 SDK (`dotnet --version`) |
| Encode works, decode fails in custom FFI code | Copy returned bytes **before** calling `bcs_free_buffer` |
| Forgotten free → leaks | Always pair encode/protect with `bcs_free_buffer`, decode/path with `bcs_free_string` |

## Related docs

- [`ffi/README.md`](../ffi/README.md) — C ABI packaging details
- [`bindings/README.md`](../bindings/README.md) — status matrix and self-test commands
- [`spec/format.md`](../spec/format.md) — binary format
- [`docs/api-reference.md`](api-reference.md) — Rust API

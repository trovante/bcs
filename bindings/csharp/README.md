# BCS C# bindings

.NET 8 P/Invoke wrapper around `libbcs_ffi`.

> Full guide (generate natives + all languages): **[docs/bindings.md](../../docs/bindings.md)**

## Prerequisites

```bash
cargo build -p bcs-ffi --release
./scripts/package-ffi.sh
# .NET 8 SDK required
dotnet --version
```

Optional:

```bash
export BCS_FFI_LIB=/absolute/path/to/libbcs_ffi.dylib
```

## Self-test

```bash
cd bindings/csharp
dotnet run --project Bcs.SelfTest
```

## Usage

```csharp
using Bcs;

var data = BcsClient.EncodeJson("""{"server":{"host":"localhost"}}""");
Console.WriteLine(BcsClient.Validate(data));
Console.WriteLine(BcsClient.GetPathJson(data, "server.host"));
```

## Projects

| Project | Purpose |
|---|---|
| `Bcs/` | Library (`BcsClient`) |
| `Bcs.SelfTest/` | Smoke test executable |

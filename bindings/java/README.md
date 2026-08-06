# BCS Java bindings

Java 22+ Foreign Function & Memory (Panama) wrapper around `libbcs_ffi`.

> Full guide (generate natives + all languages): **[docs/bindings.md](../../docs/bindings.md)**

## Prerequisites

- JDK **22+**
- Native library:

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
./bindings/java/run-selftest.sh
```

## Usage

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
```

## Notes

- Uses `System.load` + FFM downcalls (no JNI boilerplate).
- On some JDKs you may need `--enable-native-access=ALL-UNNAMED`.
- Package coordinates are illustrative (`com.trovante.bcs`); publish when ready.

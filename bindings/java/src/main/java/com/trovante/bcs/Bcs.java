package com.trovante.bcs;

import java.lang.foreign.*;
import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;
import java.lang.invoke.MethodType;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Objects;

/**
 * Java 22+ Foreign Function &amp; Memory bindings for BCS.
 */
public final class Bcs {
    public static final int OK = 0;

    public static final class BcsException extends RuntimeException {
        public final int code;

        public BcsException(int code, String message) {
            super("BCS error " + code + ": " + message);
            this.code = code;
        }
    }

    private static final Linker LINKER = Linker.nativeLinker();
    private static final SymbolLookup LIB;
    private static final MethodHandle VERSION;
    private static final MethodHandle LAST_ERROR;
    private static final MethodHandle ENCODE_JSON;
    private static final MethodHandle DECODE_TO_JSON;
    private static final MethodHandle DECODE_TO_JSON_EX;
    private static final MethodHandle GET_PATH_JSON;
    private static final MethodHandle SCHEMA_EXPORT_JSON;
    private static final MethodHandle VALIDATE;
    private static final MethodHandle PROTECT_JSON;
    private static final MethodHandle PROTECT_JSON_EX;
    private static final MethodHandle STRDUP;
    private static final MethodHandle ALLOC;
    private static final MethodHandle FREE_BUFFER;
    private static final MethodHandle FREE_STRING;

    static {
        try {
            String path = resolveLibraryPath();
            System.load(path);
            LIB = SymbolLookup.loaderLookup();

            VERSION = downcall("bcs_version", FunctionDescriptor.of(ValueLayout.ADDRESS));
            LAST_ERROR = downcall("bcs_last_error", FunctionDescriptor.of(ValueLayout.ADDRESS));
            ENCODE_JSON = downcall(
                    "bcs_encode_json",
                    FunctionDescriptor.of(
                            ValueLayout.JAVA_INT,
                            ValueLayout.ADDRESS,
                            ValueLayout.JAVA_INT,
                            ValueLayout.JAVA_INT,
                            ValueLayout.ADDRESS,
                            ValueLayout.ADDRESS));
            DECODE_TO_JSON = downcall(
                    "bcs_decode_to_json",
                    FunctionDescriptor.of(
                            ValueLayout.JAVA_INT,
                            ValueLayout.ADDRESS,
                            ValueLayout.JAVA_LONG,
                            ValueLayout.ADDRESS,
                            ValueLayout.ADDRESS));
            GET_PATH_JSON = downcall(
                    "bcs_get_path_json",
                    FunctionDescriptor.of(
                            ValueLayout.JAVA_INT,
                            ValueLayout.ADDRESS,
                            ValueLayout.JAVA_LONG,
                            ValueLayout.ADDRESS,
                            ValueLayout.ADDRESS));
            SCHEMA_EXPORT_JSON = downcall(
                    "bcs_schema_export_json",
                    FunctionDescriptor.of(
                            ValueLayout.JAVA_INT,
                            ValueLayout.ADDRESS,
                            ValueLayout.JAVA_LONG,
                            ValueLayout.ADDRESS));
            VALIDATE = downcall(
                    "bcs_validate",
                    FunctionDescriptor.of(
                            ValueLayout.JAVA_INT,
                            ValueLayout.ADDRESS,
                            ValueLayout.JAVA_LONG,
                            ValueLayout.ADDRESS));
            PROTECT_JSON = downcall(
                    "bcs_protect_json",
                    FunctionDescriptor.of(
                            ValueLayout.JAVA_INT,
                            ValueLayout.ADDRESS,
                            ValueLayout.ADDRESS,
                            ValueLayout.ADDRESS,
                            ValueLayout.JAVA_INT,
                            ValueLayout.JAVA_INT,
                            ValueLayout.ADDRESS,
                            ValueLayout.ADDRESS));
            PROTECT_JSON_EX = downcall(
                    "bcs_protect_json_ex",
                    FunctionDescriptor.of(
                            ValueLayout.JAVA_INT,
                            ValueLayout.ADDRESS,
                            ValueLayout.ADDRESS,
                            ValueLayout.ADDRESS,
                            ValueLayout.ADDRESS,
                            ValueLayout.ADDRESS,
                            ValueLayout.ADDRESS,
                            ValueLayout.ADDRESS,
                            ValueLayout.JAVA_INT,
                            ValueLayout.JAVA_INT,
                            ValueLayout.ADDRESS,
                            ValueLayout.ADDRESS));
            DECODE_TO_JSON_EX = downcall(
                    "bcs_decode_to_json_ex",
                    FunctionDescriptor.of(
                            ValueLayout.JAVA_INT,
                            ValueLayout.ADDRESS,
                            ValueLayout.JAVA_LONG,
                            ValueLayout.ADDRESS,
                            ValueLayout.ADDRESS,
                            ValueLayout.ADDRESS,
                            ValueLayout.ADDRESS,
                            ValueLayout.ADDRESS,
                            ValueLayout.ADDRESS));
            STRDUP = downcall(
                    "bcs_strdup",
                    FunctionDescriptor.of(ValueLayout.ADDRESS, ValueLayout.ADDRESS));
            ALLOC = downcall(
                    "bcs_alloc",
                    FunctionDescriptor.of(ValueLayout.ADDRESS, ValueLayout.JAVA_LONG));
            FREE_BUFFER = downcall(
                    "bcs_free_buffer",
                    FunctionDescriptor.ofVoid(ValueLayout.ADDRESS, ValueLayout.JAVA_LONG));
            FREE_STRING = downcall(
                    "bcs_free_string",
                    FunctionDescriptor.ofVoid(ValueLayout.ADDRESS));
        } catch (Throwable t) {
            throw new ExceptionInInitializerError(t);
        }
    }

    private static MethodHandle downcall(String name, FunctionDescriptor desc) {
        MemorySegment sym = LIB.find(name)
                .orElseThrow(() -> new UnsatisfiedLinkError("Missing symbol: " + name));
        return LINKER.downcallHandle(sym, desc);
    }

    private static String resolveLibraryPath() {
        String env = System.getenv("BCS_FFI_LIB");
        if (env != null && !env.isBlank() && Files.isRegularFile(Path.of(env))) {
            return Path.of(env).toAbsolutePath().toString();
        }

        Path root = findRepoRoot();
        String os = osName();
        String arch = archName();
        String file = libraryFileName();

        List<Path> candidates = new ArrayList<>();
        candidates.add(root.resolve("dist/ffi/" + os + "-" + arch).resolve(file));
        candidates.add(root.resolve("target/release").resolve(file));
        candidates.add(root.resolve("target/debug").resolve(file));
        String cargoTarget = System.getenv("CARGO_TARGET_DIR");
        if (cargoTarget != null && !cargoTarget.isBlank()) {
            candidates.add(Path.of(cargoTarget, "release", file));
            candidates.add(Path.of(cargoTarget, "debug", file));
        }

        for (Path p : candidates) {
            if (Files.isRegularFile(p)) {
                return p.toAbsolutePath().toString();
            }
        }
        throw new IllegalStateException("Could not find " + file + ". Build bcs-ffi or set BCS_FFI_LIB.");
    }

    private static Path findRepoRoot() {
        Path dir = Path.of("").toAbsolutePath();
        while (dir != null) {
            if (Files.isRegularFile(dir.resolve("Cargo.toml"))
                    && Files.isDirectory(dir.resolve("ffi"))) {
                return dir;
            }
            dir = dir.getParent();
        }
        // bindings/java -> repo root
        return Path.of("").toAbsolutePath().resolve("../..").normalize();
    }

    private static String osName() {
        String os = System.getProperty("os.name").toLowerCase();
        if (os.contains("mac")) return "darwin";
        if (os.contains("win")) return "windows";
        return "linux";
    }

    private static String archName() {
        String arch = System.getProperty("os.arch").toLowerCase();
        if (arch.contains("aarch64") || arch.contains("arm64")) return "arm64";
        return "x64";
    }

    private static String libraryFileName() {
        String os = osName();
        if (os.equals("darwin")) return "libbcs_ffi.dylib";
        if (os.equals("windows")) return "bcs_ffi.dll";
        return "libbcs_ffi.so";
    }

    private Bcs() {}

    public static String version() {
        try {
            MemorySegment ptr = (MemorySegment) VERSION.invoke();
            if (ptr.equals(MemorySegment.NULL)) return "";
            return ptr.reinterpret(Long.MAX_VALUE).getString(0);
        } catch (Throwable t) {
            throw new BcsException(Bcs.OK, t.getMessage());
        }
    }

    public static byte[] encodeJson(String json) {
        return encodeJson(json, false, false);
    }

    public static byte[] encodeJson(String json, boolean compact, boolean compressData) {
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment jsonSeg = arena.allocateFrom(json);
            MemorySegment outPtr = arena.allocate(ValueLayout.ADDRESS);
            MemorySegment outLen = arena.allocate(ValueLayout.JAVA_LONG);
            int code = (int) ENCODE_JSON.invoke(
                    jsonSeg,
                    compact ? 1 : 0,
                    compressData ? 1 : 0,
                    outPtr,
                    outLen);
            check(code);
            MemorySegment ptr = outPtr.get(ValueLayout.ADDRESS, 0);
            long len = outLen.get(ValueLayout.JAVA_LONG, 0);
            try {
                return ptr.reinterpret(len).toArray(ValueLayout.JAVA_BYTE);
            } finally {
                FREE_BUFFER.invoke(ptr, len);
            }
        } catch (BcsException e) {
            throw e;
        } catch (Throwable t) {
            throw new BcsException(-1, Objects.toString(t.getMessage(), t.toString()));
        }
    }

    public static String decodeToJson(byte[] data) {
        return decodeToJson(data, null);
    }

    public static String decodeToJson(byte[] data, String password) {
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment dataSeg = arena.allocateFrom(ValueLayout.JAVA_BYTE, data);
            MemorySegment passwordSeg = password == null ? MemorySegment.NULL : arena.allocateFrom(password);
            MemorySegment outJson = arena.allocate(ValueLayout.ADDRESS);
            int code = (int) DECODE_TO_JSON.invoke(dataSeg, (long) data.length, passwordSeg, outJson);
            check(code);
            MemorySegment ptr = outJson.get(ValueLayout.ADDRESS, 0);
            try {
                return ptr.reinterpret(Long.MAX_VALUE).getString(0);
            } finally {
                FREE_STRING.invoke(ptr);
            }
        } catch (BcsException e) {
            throw e;
        } catch (Throwable t) {
            throw new BcsException(-1, Objects.toString(t.getMessage(), t.toString()));
        }
    }

    @FunctionalInterface
    public interface SecretResolveFn {
        String resolve(String scheme, String locator);
    }

    @FunctionalInterface
    public interface KeyUnwrapFn {
        byte[] unwrap(String provider, String kekLocator, byte[] wrapped);
    }

    public static String decodeToJsonEx(byte[] data, String password, SecretResolveFn resolveSecrets, KeyUnwrapFn unwrapKey) {
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment dataSeg = arena.allocateFrom(ValueLayout.JAVA_BYTE, data);
            MemorySegment passwordSeg = password == null ? MemorySegment.NULL : arena.allocateFrom(password);
            MemorySegment outJson = arena.allocate(ValueLayout.ADDRESS);

            // Create resolve callback
            MemorySegment resolveFn = MemorySegment.NULL;
            if (resolveSecrets != null) {
                var resolveDesc = FunctionDescriptor.of(ValueLayout.ADDRESS, ValueLayout.ADDRESS, ValueLayout.ADDRESS, ValueLayout.ADDRESS);
                MethodHandle resolveHandle = MethodHandles.lookup().findStatic(
                        Bcs.class,
                        "resolveTrampoline",
                        MethodType.methodType(
                                MemorySegment.class,
                                SecretResolveFn.class,
                                MemorySegment.class,
                                MemorySegment.class,
                                MemorySegment.class))
                        .bindTo(resolveSecrets);
                var resolveStub = LINKER.upcallStub(
                        resolveHandle,
                        resolveDesc, arena);
                resolveFn = resolveStub;
            }

            // Create unwrap callback
            MemorySegment unwrapFn = MemorySegment.NULL;
            if (unwrapKey != null) {
                var unwrapDesc = FunctionDescriptor.of(ValueLayout.JAVA_INT,
                        ValueLayout.ADDRESS, ValueLayout.ADDRESS,
                        ValueLayout.ADDRESS, ValueLayout.JAVA_LONG,
                        ValueLayout.ADDRESS, ValueLayout.ADDRESS);
                MethodHandle unwrapHandle = MethodHandles.lookup().findStatic(
                        Bcs.class,
                        "unwrapTrampoline",
                        MethodType.methodType(
                                int.class,
                                KeyUnwrapFn.class,
                                MemorySegment.class,
                                MemorySegment.class,
                                MemorySegment.class,
                                long.class,
                                MemorySegment.class,
                                MemorySegment.class))
                        .bindTo(unwrapKey);
                var unwrapStub = LINKER.upcallStub(
                        unwrapHandle,
                        unwrapDesc, arena);
                unwrapFn = unwrapStub;
            }

            int code = (int) DECODE_TO_JSON_EX.invoke(
                    dataSeg, (long) data.length, passwordSeg,
                    resolveFn, MemorySegment.NULL,
                    unwrapFn, MemorySegment.NULL,
                    outJson);
            check(code);
            MemorySegment ptr = outJson.get(ValueLayout.ADDRESS, 0);
            try {
                return ptr.reinterpret(Long.MAX_VALUE).getString(0);
            } finally {
                FREE_STRING.invoke(ptr);
            }
        } catch (BcsException e) {
            throw e;
        } catch (Throwable t) {
            throw new BcsException(-1, Objects.toString(t.getMessage(), t.toString()));
        }
    }

    private static MemorySegment resolveTrampoline(SecretResolveFn fn, MemorySegment schemePtr, MemorySegment locatorPtr, MemorySegment ud) {
        try {
            String scheme = schemePtr.reinterpret(Long.MAX_VALUE).getString(0);
            String locator = locatorPtr.reinterpret(Long.MAX_VALUE).getString(0);
            String result = fn.resolve(scheme, locator);
            if (result == null) return MemorySegment.NULL;
            try (Arena callbackArena = Arena.ofConfined()) {
                return (MemorySegment) STRDUP.invoke(callbackArena.allocateFrom(result));
            }
        } catch (Throwable t) {
            return MemorySegment.NULL;
        }
    }

    private static int unwrapTrampoline(KeyUnwrapFn fn, MemorySegment providerPtr, MemorySegment kekLocatorPtr,
                                        MemorySegment wrappedPtr, long wrappedLen, MemorySegment outDek, MemorySegment ud) {
        try {
            String provider = providerPtr.reinterpret(Long.MAX_VALUE).getString(0);
            String kekLocator = kekLocatorPtr.reinterpret(Long.MAX_VALUE).getString(0);
            byte[] wrapped = wrappedPtr.reinterpret(wrappedLen).toArray(ValueLayout.JAVA_BYTE);
            byte[] result = fn.unwrap(provider, kekLocator, wrapped);
            if (result == null || result.length != 32) return 1;
            outDek.reinterpret(32).copyFrom(MemorySegment.ofArray(result));
            return 0;
        } catch (Throwable t) {
            return 1;
        }
    }

    public static String getPathJson(byte[] data, String path) {
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment dataSeg = arena.allocateFrom(ValueLayout.JAVA_BYTE, data);
            MemorySegment pathSeg = arena.allocateFrom(path);
            MemorySegment outJson = arena.allocate(ValueLayout.ADDRESS);
            int code = (int) GET_PATH_JSON.invoke(dataSeg, (long) data.length, pathSeg, outJson);
            check(code);
            MemorySegment ptr = outJson.get(ValueLayout.ADDRESS, 0);
            try {
                return ptr.reinterpret(Long.MAX_VALUE).getString(0);
            } finally {
                FREE_STRING.invoke(ptr);
            }
        } catch (BcsException e) {
            throw e;
        } catch (Throwable t) {
            throw new BcsException(-1, Objects.toString(t.getMessage(), t.toString()));
        }
    }

    /** Export agent-safe schema JSON (paths/types/sensitive; never data-layer values). */
    public static String schemaExportJson(byte[] data) {
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment dataSeg = arena.allocateFrom(ValueLayout.JAVA_BYTE, data);
            MemorySegment outJson = arena.allocate(ValueLayout.ADDRESS);
            int code = (int) SCHEMA_EXPORT_JSON.invoke(dataSeg, (long) data.length, outJson);
            check(code);
            MemorySegment ptr = outJson.get(ValueLayout.ADDRESS, 0);
            try {
                return ptr.reinterpret(Long.MAX_VALUE).getString(0);
            } finally {
                FREE_STRING.invoke(ptr);
            }
        } catch (BcsException e) {
            throw e;
        } catch (Throwable t) {
            throw new BcsException(-1, Objects.toString(t.getMessage(), t.toString()));
        }
    }

    public static boolean validate(byte[] data) {
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment dataSeg = arena.allocateFrom(ValueLayout.JAVA_BYTE, data);
            MemorySegment outOk = arena.allocate(ValueLayout.JAVA_INT);
            int code = (int) VALIDATE.invoke(dataSeg, (long) data.length, outOk);
            check(code);
            return outOk.get(ValueLayout.JAVA_INT, 0) == 1;
        } catch (BcsException e) {
            throw e;
        } catch (Throwable t) {
            throw new BcsException(-1, Objects.toString(t.getMessage(), t.toString()));
        }
    }

    public static byte[] protectJson(String json, List<String> paths, String password) {
        return protectJson(json, paths, password, false, false);
    }

    public static byte[] protectJson(
            String json,
            List<String> paths,
            String password,
            boolean compact,
            boolean compressData) {
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment jsonSeg = arena.allocateFrom(json);
            MemorySegment pathsSeg = arena.allocateFrom(String.join(",", paths));
            MemorySegment passwordSeg = arena.allocateFrom(password);
            MemorySegment outPtr = arena.allocate(ValueLayout.ADDRESS);
            MemorySegment outLen = arena.allocate(ValueLayout.JAVA_LONG);
            int code = (int) PROTECT_JSON.invoke(
                    jsonSeg,
                    pathsSeg,
                    passwordSeg,
                    compact ? 1 : 0,
                    compressData ? 1 : 0,
                    outPtr,
                    outLen);
            check(code);
            MemorySegment ptr = outPtr.get(ValueLayout.ADDRESS, 0);
            long len = outLen.get(ValueLayout.JAVA_LONG, 0);
            try {
                return ptr.reinterpret(len).toArray(ValueLayout.JAVA_BYTE);
            } finally {
                FREE_BUFFER.invoke(ptr, len);
            }
        } catch (BcsException e) {
            throw e;
        } catch (Throwable t) {
            throw new BcsException(-1, Objects.toString(t.getMessage(), t.toString()));
        }
    }

    @FunctionalInterface
    public interface KeyWrapFn {
        byte[] wrap(String provider, String kekLocator, byte[] dek);
    }

    public static byte[] protectJsonEx(
            String json,
            List<String> paths,
            String password,
            String kmsProvider,
            String kmsKey,
            KeyWrapFn wrapKey,
            boolean compact,
            boolean compressData) {
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment jsonSeg = arena.allocateFrom(json);
            MemorySegment pathsSeg = arena.allocateFrom(String.join(",", paths));
            MemorySegment passwordSeg = password == null ? MemorySegment.NULL : arena.allocateFrom(password);
            MemorySegment providerSeg = kmsProvider == null ? MemorySegment.NULL : arena.allocateFrom(kmsProvider);
            MemorySegment keySeg = kmsKey == null ? MemorySegment.NULL : arena.allocateFrom(kmsKey);
            MemorySegment outPtr = arena.allocate(ValueLayout.ADDRESS);
            MemorySegment outLen = arena.allocate(ValueLayout.JAVA_LONG);

            // Create wrap callback
            MemorySegment wrapFn = MemorySegment.NULL;
            if (wrapKey != null && kmsProvider != null && kmsKey != null) {
                var wrapDesc = FunctionDescriptor.of(ValueLayout.JAVA_INT,
                        ValueLayout.ADDRESS, ValueLayout.ADDRESS,
                        ValueLayout.ADDRESS, ValueLayout.JAVA_LONG,
                        ValueLayout.ADDRESS, ValueLayout.ADDRESS,
                        ValueLayout.ADDRESS);
                MethodHandle wrapHandle = MethodHandles.lookup().findStatic(
                        Bcs.class,
                        "wrapTrampoline",
                        MethodType.methodType(
                                int.class,
                                KeyWrapFn.class,
                                MemorySegment.class,
                                MemorySegment.class,
                                MemorySegment.class,
                                long.class,
                                MemorySegment.class,
                                MemorySegment.class,
                                MemorySegment.class))
                        .bindTo(wrapKey);
                var wrapStub = LINKER.upcallStub(
                        wrapHandle,
                        wrapDesc, arena);
                wrapFn = wrapStub;
            }

            int code = (int) PROTECT_JSON_EX.invoke(
                    jsonSeg, pathsSeg, passwordSeg, providerSeg, keySeg,
                    wrapFn, MemorySegment.NULL,
                    compact ? 1 : 0, compressData ? 1 : 0,
                    outPtr, outLen);
            check(code);
            MemorySegment ptr = outPtr.get(ValueLayout.ADDRESS, 0);
            long len = outLen.get(ValueLayout.JAVA_LONG, 0);
            try {
                return ptr.reinterpret(len).toArray(ValueLayout.JAVA_BYTE);
            } finally {
                FREE_BUFFER.invoke(ptr, len);
            }
        } catch (BcsException e) {
            throw e;
        } catch (Throwable t) {
            throw new BcsException(-1, Objects.toString(t.getMessage(), t.toString()));
        }
    }

    private static int wrapTrampoline(KeyWrapFn fn, MemorySegment providerPtr, MemorySegment kekLocatorPtr,
                                      MemorySegment dekPtr, long dekLen, MemorySegment outWrappedPtr,
                                      MemorySegment outWrappedLenPtr, MemorySegment ud) {
        try {
            String provider = providerPtr.reinterpret(Long.MAX_VALUE).getString(0);
            String kekLocator = kekLocatorPtr.reinterpret(Long.MAX_VALUE).getString(0);
            byte[] dek = dekPtr.reinterpret(dekLen).toArray(ValueLayout.JAVA_BYTE);
            byte[] result = fn.wrap(provider, kekLocator, dek);
            if (result == null) return 1;
            MemorySegment buf = (MemorySegment) ALLOC.invoke((long) result.length);
            buf.reinterpret(result.length).copyFrom(MemorySegment.ofArray(result));
            outWrappedPtr.set(ValueLayout.ADDRESS, 0, buf);
            outWrappedLenPtr.set(ValueLayout.JAVA_LONG, 0, (long) result.length);
            return 0;
        } catch (Throwable t) {
            return 1;
        }
    }

    private static void check(int code) throws Throwable {
        if (code == OK) {
            return;
        }
        MemorySegment err = (MemorySegment) LAST_ERROR.invoke();
        String message = err.equals(MemorySegment.NULL)
                ? "unknown error"
                : err.reinterpret(Long.MAX_VALUE).getString(0);
        throw new BcsException(code, message);
    }

    /** Smoke test entrypoint: {@code java ... com.trovante.bcs.Bcs}. */
    public static void main(String[] args) {
        byte[] data = encodeJson(
                "{\"server\":{\"host\":\"localhost\"},\"database\":{\"password\":\"secret\"}}");
        if (!validate(data)) {
            throw new IllegalStateException("validate failed");
        }
        String host = getPathJson(data, "server.host");
        if (!"\"localhost\"".equals(host)) {
            throw new IllegalStateException("unexpected host: " + host);
        }
        String schema = schemaExportJson(data);
        if (schema.contains("secret")) {
            throw new IllegalStateException("agent-safe schema leaked value: " + schema);
        }
        if (!schema.contains("database") && !schema.contains("password")) {
            throw new IllegalStateException("expected schema paths, got: " + schema);
        }
        byte[] protectedBytes = protectJson(
                "{\"database\":{\"password\":\"secret\"}}",
                List.of("database.password"),
                "master");
        String masked = decodeToJson(protectedBytes);
        if (!masked.contains("[PROTECTED]")) {
            throw new IllegalStateException("expected masked output: " + masked);
        }
        String revealed = decodeToJson(protectedBytes, "master");
        if (!revealed.contains("secret")) {
            throw new IllegalStateException("expected revealed password: " + revealed);
        }

        byte[] secretRef = encodeJson(
                "{\"token\":\"__bcs_secret_ref__:env:API_TOKEN\"}");
        String maskedRef = decodeToJsonEx(secretRef, null, null, null);
        if (!maskedRef.contains("[SECRET_REF]")) {
            throw new IllegalStateException("expected masked secret reference: " + maskedRef);
        }
        String resolvedRef = decodeToJsonEx(
                secretRef,
                null,
                (scheme, locator) ->
                        scheme.equals("env") && locator.equals("API_TOKEN") ? "java-token" : null,
                null);
        if (!resolvedRef.contains("java-token")) {
            throw new IllegalStateException("expected resolved secret reference: " + resolvedRef);
        }

        System.out.println("bcs java bindings ok (version=" + version() + ")");
    }
}

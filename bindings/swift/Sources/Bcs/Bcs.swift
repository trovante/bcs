import Darwin
import Foundation

public enum BcsStatus {
    public static let ok = 0
}

public struct BcsError: Error, CustomStringConvertible {
    public let code: Int32
    public let message: String

    public init(code: Int32, message: String) {
        self.code = code
        self.message = message
    }

    public var description: String {
        "BCS error \(code): \(message)"
    }
}

private enum Native {
    typealias VersionFn = @convention(c) () -> UnsafePointer<CChar>?
    typealias LastErrorFn = @convention(c) () -> UnsafePointer<CChar>?
    typealias EncodeJsonFn = @convention(c) (
        UnsafePointer<CChar>?,
        Int32,
        Int32,
        UnsafeMutablePointer<UnsafeMutablePointer<UInt8>?>?,
        UnsafeMutablePointer<UInt>?
    ) -> Int32
    typealias DecodeToJsonFn = @convention(c) (
        UnsafePointer<UInt8>?,
        UInt,
        UnsafePointer<CChar>?,
        UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?
    ) -> Int32
    typealias DecodeToJsonExFn = @convention(c) (
        UnsafePointer<UInt8>?,
        UInt,
        UnsafePointer<CChar>?,
        UnsafeRawPointer?,
        UnsafeMutableRawPointer?,
        UnsafeRawPointer?,
        UnsafeMutableRawPointer?,
        UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?
    ) -> Int32
    typealias GetPathJsonFn = @convention(c) (
        UnsafePointer<UInt8>?,
        UInt,
        UnsafePointer<CChar>?,
        UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?
    ) -> Int32
    typealias SchemaExportJsonFn = @convention(c) (
        UnsafePointer<UInt8>?,
        UInt,
        UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?
    ) -> Int32
    typealias ValidateFn = @convention(c) (
        UnsafePointer<UInt8>?,
        UInt,
        UnsafeMutablePointer<Int32>?
    ) -> Int32
    typealias ProtectJsonFn = @convention(c) (
        UnsafePointer<CChar>?,
        UnsafePointer<CChar>?,
        UnsafePointer<CChar>?,
        Int32,
        Int32,
        UnsafeMutablePointer<UnsafeMutablePointer<UInt8>?>?,
        UnsafeMutablePointer<UInt>?
    ) -> Int32
    typealias ProtectJsonExFn = @convention(c) (
        UnsafePointer<CChar>?,
        UnsafePointer<CChar>?,
        UnsafePointer<CChar>?,
        UnsafePointer<CChar>?,
        UnsafePointer<CChar>?,
        UnsafeRawPointer?,
        UnsafeMutableRawPointer?,
        Int32,
        Int32,
        UnsafeMutablePointer<UnsafeMutablePointer<UInt8>?>?,
        UnsafeMutablePointer<UInt>?
    ) -> Int32
    typealias StrdupFn = @convention(c) (UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>?
    typealias AllocFn = @convention(c) (UInt) -> UnsafeMutablePointer<UInt8>?
    typealias FreeBufferFn = @convention(c) (UnsafeMutablePointer<UInt8>?, UInt) -> Void
    typealias FreeStringFn = @convention(c) (UnsafeMutablePointer<CChar>?) -> Void

    static let handle: UnsafeMutableRawPointer = {
        let path = resolveLibraryPath()
        guard let h = dlopen(path, RTLD_NOW | RTLD_LOCAL) else {
            let err = String(cString: dlerror())
            fatalError("Could not load bcs_ffi at \(path): \(err)")
        }
        return h
    }()

    static let version: VersionFn = load("bcs_version")
    static let lastError: LastErrorFn = load("bcs_last_error")
    static let encodeJson: EncodeJsonFn = load("bcs_encode_json")
    static let decodeToJson: DecodeToJsonFn = load("bcs_decode_to_json")
    static let decodeToJsonEx: DecodeToJsonExFn = load("bcs_decode_to_json_ex")
    static let getPathJson: GetPathJsonFn = load("bcs_get_path_json")
    static let schemaExportJson: SchemaExportJsonFn = load("bcs_schema_export_json")
    static let validate: ValidateFn = load("bcs_validate")
    static let protectJson: ProtectJsonFn = load("bcs_protect_json")
    static let protectJsonEx: ProtectJsonExFn = load("bcs_protect_json_ex")
    static let strdup: StrdupFn = load("bcs_strdup")
    static let alloc: AllocFn = load("bcs_alloc")
    static let freeBuffer: FreeBufferFn = load("bcs_free_buffer")
    static let freeString: FreeStringFn = load("bcs_free_string")

    private static func load<T>(_ name: String) -> T {
        guard let sym = dlsym(handle, name) else {
            fatalError("Missing symbol \(name)")
        }
        return unsafeBitCast(sym, to: T.self)
    }

    private static func resolveLibraryPath() -> String {
        if let env = ProcessInfo.processInfo.environment["BCS_FFI_LIB"],
           FileManager.default.fileExists(atPath: env)
        {
            return env
        }

        let root = findRepoRoot()
        let (os, arch, fileName) = platformTriplet()
        var candidates = [
            root.appendingPathComponent("dist/ffi/\(os)-\(arch)/\(fileName)").path,
            root.appendingPathComponent("target/release/\(fileName)").path,
            root.appendingPathComponent("target/debug/\(fileName)").path,
        ]
        if let cargo = ProcessInfo.processInfo.environment["CARGO_TARGET_DIR"] {
            candidates.append((cargo as NSString).appendingPathComponent("release/\(fileName)"))
            candidates.append((cargo as NSString).appendingPathComponent("debug/\(fileName)"))
        }
        for path in candidates where FileManager.default.fileExists(atPath: path) {
            return path
        }
        fatalError("Could not find \(fileName). Build bcs-ffi or set BCS_FFI_LIB.")
    }

    private static func findRepoRoot() -> URL {
        var url = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
        // Sources/Bcs -> swift -> bindings -> repo
        for _ in 0..<6 {
            let cargo = url.appendingPathComponent("Cargo.toml")
            let ffi = url.appendingPathComponent("ffi")
            if FileManager.default.fileExists(atPath: cargo.path),
               FileManager.default.fileExists(atPath: ffi.path)
            {
                return url
            }
            url.deleteLastPathComponent()
        }
        return URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
    }

    private static func platformTriplet() -> (String, String, String) {
        #if os(macOS)
        let os = "darwin"
        let file = "libbcs_ffi.dylib"
        #elseif os(Linux)
        let os = "linux"
        let file = "libbcs_ffi.so"
        #else
        let os = "unknown"
        let file = "libbcs_ffi.so"
        #endif

        #if arch(arm64)
        let arch = "arm64"
        #else
        let arch = "x64"
        #endif
        return (os, arch, file)
    }
}

public enum Bcs {
    public static func version() -> String {
        guard let ptr = Native.version() else { return "" }
        return String(cString: ptr)
    }

    public static func encodeJson(
        _ json: String,
        compact: Bool = false,
        compressData: Bool = false
    ) throws -> Data {
        var outPtr: UnsafeMutablePointer<UInt8>?
        var outLen: UInt = 0
        let code = json.withCString { cJson in
            Native.encodeJson(
                cJson,
                compact ? 1 : 0,
                compressData ? 1 : 0,
                &outPtr,
                &outLen
            )
        }
        try check(code)
        defer { Native.freeBuffer(outPtr, outLen) }
        guard let outPtr else { return Data() }
        return Data(bytes: outPtr, count: Int(outLen))
    }

    public static func decodeToJson(_ data: Data, password: String? = nil) throws -> String {
        var outJson: UnsafeMutablePointer<CChar>?
        let code: Int32 = data.withUnsafeBytes { raw in
            let base = raw.bindMemory(to: UInt8.self).baseAddress
            if let password {
                return password.withCString { cPass in
                    Native.decodeToJson(base, UInt(data.count), cPass, &outJson)
                }
            }
            return Native.decodeToJson(base, UInt(data.count), nil, &outJson)
        }
        try check(code)
        defer { Native.freeString(outJson) }
        guard let outJson else { return "" }
        return String(cString: outJson)
    }

    public typealias SecretResolveFn = (String, String) -> String?
    public typealias KeyUnwrapFn = (String, String, Data) -> Data?

    public static func decodeToJsonEx(
        _ data: Data,
        password: String? = nil,
        resolveSecrets: SecretResolveFn? = nil,
        unwrapKey: KeyUnwrapFn? = nil
    ) throws -> String {
        var outJson: UnsafeMutablePointer<CChar>?

        // Trampoline context for callbacks
        class CallbackContext {
            var resolveFn: SecretResolveFn?
            var unwrapFn: KeyUnwrapFn?
        }
        let ctx = CallbackContext()
        ctx.resolveFn = resolveSecrets
        ctx.unwrapFn = unwrapKey

        let resolveTrampoline: @convention(c) (
            UnsafePointer<CChar>?,
            UnsafePointer<CChar>?,
            UnsafeMutableRawPointer?
        ) -> UnsafeMutablePointer<CChar>? = { schemePtr, locatorPtr, ud in
            guard let ud = ud, let schemePtr = schemePtr, let locatorPtr = locatorPtr else {
                return nil
            }
            let ctx = Unmanaged<CallbackContext>.fromOpaque(ud).takeUnretainedValue()
            guard let fn = ctx.resolveFn else { return nil }
            let scheme = String(cString: schemePtr)
            let locator = String(cString: locatorPtr)
            guard let result = fn(scheme, locator) else { return nil }
            return result.withCString { cStr in Native.strdup(cStr) }
        }

        let unwrapTrampoline: @convention(c) (
            UnsafePointer<CChar>?,
            UnsafePointer<CChar>?,
            UnsafePointer<UInt8>?,
            UInt,
            UnsafeMutablePointer<UInt8>?,
            UnsafeMutableRawPointer?
        ) -> Int32 = { providerPtr, kekLocatorPtr, wrappedPtr, wrappedLen, outDek, ud in
            guard let ud = ud,
                  let providerPtr = providerPtr,
                  let kekLocatorPtr = kekLocatorPtr,
                  let wrappedPtr = wrappedPtr,
                  let outDek = outDek
            else {
                return 1
            }
            let ctx = Unmanaged<CallbackContext>.fromOpaque(ud).takeUnretainedValue()
            guard let fn = ctx.unwrapFn else { return 1 }
            let provider = String(cString: providerPtr)
            let kekLocator = String(cString: kekLocatorPtr)
            let wrapped = Data(bytes: wrappedPtr, count: Int(wrappedLen))
            guard let result = fn(provider, kekLocator, wrapped), result.count == 32 else {
                return 1
            }
            result.copyBytes(to: outDek, count: 32)
            return 0
        }

        let ctxPtr = Unmanaged.passRetained(ctx).toOpaque()
        defer { Unmanaged<CallbackContext>.fromOpaque(ctxPtr).release() }

        let resolveFnPtr = resolveSecrets.map {
            _ in unsafeBitCast(resolveTrampoline, to: UnsafeRawPointer.self)
        }
        let unwrapFnPtr = unwrapKey.map {
            _ in unsafeBitCast(unwrapTrampoline, to: UnsafeRawPointer.self)
        }
        let resolveCtxPtr = resolveSecrets == nil ? nil : ctxPtr
        let unwrapCtxPtr = unwrapKey == nil ? nil : ctxPtr

        let code: Int32 = data.withUnsafeBytes { raw in
            let base = raw.bindMemory(to: UInt8.self).baseAddress
            if let password {
                return password.withCString { cPass in
                    Native.decodeToJsonEx(
                        base, UInt(data.count), cPass,
                        resolveFnPtr, resolveCtxPtr,
                        unwrapFnPtr, unwrapCtxPtr,
                        &outJson
                    )
                }
            }
            return Native.decodeToJsonEx(
                base, UInt(data.count), nil,
                resolveFnPtr, resolveCtxPtr,
                unwrapFnPtr, unwrapCtxPtr,
                &outJson
            )
        }
        try check(code)
        defer { Native.freeString(outJson) }
        guard let outJson else { return "" }
        return String(cString: outJson)
    }

    public static func getPathJson(_ data: Data, path: String) throws -> String {
        var outJson: UnsafeMutablePointer<CChar>?
        let code: Int32 = data.withUnsafeBytes { raw in
            let base = raw.bindMemory(to: UInt8.self).baseAddress
            return path.withCString { cPath in
                Native.getPathJson(base, UInt(data.count), cPath, &outJson)
            }
        }
        try check(code)
        defer { Native.freeString(outJson) }
        guard let outJson else { return "" }
        return String(cString: outJson)
    }

    /// Export agent-safe schema JSON (paths/types/sensitive; never data-layer values).
    public static func schemaExportJson(_ data: Data) throws -> String {
        var outJson: UnsafeMutablePointer<CChar>?
        let code: Int32 = data.withUnsafeBytes { raw in
            let base = raw.bindMemory(to: UInt8.self).baseAddress
            return Native.schemaExportJson(base, UInt(data.count), &outJson)
        }
        try check(code)
        defer { Native.freeString(outJson) }
        guard let outJson else { return "" }
        return String(cString: outJson)
    }

    public static func validate(_ data: Data) throws -> Bool {
        var ok: Int32 = 0
        let code: Int32 = data.withUnsafeBytes { raw in
            let base = raw.bindMemory(to: UInt8.self).baseAddress
            return Native.validate(base, UInt(data.count), &ok)
        }
        try check(code)
        return ok == 1
    }

    public static func protectJson(
        _ json: String,
        paths: [String],
        password: String,
        compact: Bool = false,
        compressData: Bool = false
    ) throws -> Data {
        var outPtr: UnsafeMutablePointer<UInt8>?
        var outLen: UInt = 0
        let csv = paths.joined(separator: ",")
        let code = json.withCString { cJson in
            csv.withCString { cPaths in
                password.withCString { cPass in
                    Native.protectJson(
                        cJson,
                        cPaths,
                        cPass,
                        compact ? 1 : 0,
                        compressData ? 1 : 0,
                        &outPtr,
                        &outLen
                    )
                }
            }
        }
        try check(code)
        defer { Native.freeBuffer(outPtr, outLen) }
        guard let outPtr else { return Data() }
        return Data(bytes: outPtr, count: Int(outLen))
    }

    public typealias KeyWrapFn = (String, String, Data) -> Data?

    public static func protectJsonEx(
        _ json: String,
        paths: [String],
        password: String? = nil,
        kmsProvider: String? = nil,
        kmsKey: String? = nil,
        wrapKey: KeyWrapFn? = nil,
        compact: Bool = false,
        compressData: Bool = false
    ) throws -> Data {
        var outPtr: UnsafeMutablePointer<UInt8>?
        var outLen: UInt = 0
        let csv = paths.joined(separator: ",")

        class WrapContext {
            var fn: KeyWrapFn?
        }
        let wrapCtx = WrapContext()
        wrapCtx.fn = wrapKey

        let wrapTrampoline: @convention(c) (
            UnsafePointer<CChar>?,
            UnsafePointer<CChar>?,
            UnsafePointer<UInt8>?,
            UInt,
            UnsafeMutablePointer<UnsafeMutablePointer<UInt8>?>?,
            UnsafeMutablePointer<UInt>?,
            UnsafeMutableRawPointer?
        ) -> Int32 = { providerPtr, kekLocatorPtr, dekPtr, dekLen, outWrappedPtr, outWrappedLenPtr, ud in
            guard let ud = ud,
                  let providerPtr = providerPtr,
                  let kekLocatorPtr = kekLocatorPtr,
                  let dekPtr = dekPtr,
                  let outWrappedPtr = outWrappedPtr,
                  let outWrappedLenPtr = outWrappedLenPtr
            else {
                return 1
            }
            let ctx = Unmanaged<WrapContext>.fromOpaque(ud).takeUnretainedValue()
            guard let fn = ctx.fn else { return 1 }
            let provider = String(cString: providerPtr)
            let kekLocator = String(cString: kekLocatorPtr)
            let dek = Data(bytes: dekPtr, count: Int(dekLen))
            guard let result = fn(provider, kekLocator, dek) else { return 1 }
            let buf = Native.alloc(UInt(result.count))
            guard let buf else { return 1 }
            result.copyBytes(to: buf, count: result.count)
            outWrappedPtr.pointee = buf
            outWrappedLenPtr.pointee = UInt(result.count)
            return 0
        }

        let ctxPtr = Unmanaged.passRetained(wrapCtx).toOpaque()
        defer { Unmanaged<WrapContext>.fromOpaque(ctxPtr).release() }

        let hasKmsCallback = wrapKey != nil && kmsProvider != nil && kmsKey != nil
        let wrapFnPtr = hasKmsCallback
            ? unsafeBitCast(wrapTrampoline, to: UnsafeRawPointer.self)
            : nil
        let wrapCtxPtr = hasKmsCallback ? ctxPtr : nil

        let code = json.withCString { cJson in
            csv.withCString { cPaths in
                withOptionalCString(password) { cPass in
                    withOptionalCString(kmsProvider) { cProvider in
                        withOptionalCString(kmsKey) { cKey in
                            Native.protectJsonEx(
                                cJson, cPaths, cPass, cProvider, cKey,
                                wrapFnPtr, wrapCtxPtr,
                                compact ? 1 : 0,
                                compressData ? 1 : 0,
                                &outPtr, &outLen
                            )
                        }
                    }
                }
            }
        }
        try check(code)
        defer { Native.freeBuffer(outPtr, outLen) }
        guard let outPtr else { return Data() }
        return Data(bytes: outPtr, count: Int(outLen))
    }

    private static func withOptionalCString<Result>(
        _ value: String?,
        _ body: (UnsafePointer<CChar>?) throws -> Result
    ) rethrows -> Result {
        if let value {
            return try value.withCString(body)
        }
        return try body(nil)
    }

    private static func check(_ code: Int32) throws {
        guard code != BcsStatus.ok else { return }
        let message: String
        if let ptr = Native.lastError() {
            message = String(cString: ptr)
        } else {
            message = "unknown error"
        }
        throw BcsError(code: code, message: message)
    }
}

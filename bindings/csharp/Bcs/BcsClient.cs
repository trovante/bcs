using System.Reflection;
using System.Runtime.InteropServices;
using System.Text;

namespace Bcs;

/// <summary>Status codes mirrored from <c>bcs.h</c>.</summary>
public static class BcsStatus
{
    public const int Ok = 0;
    public const int ErrNull = 1;
    public const int ErrUtf8 = 2;
    public const int ErrFormat = 3;
    public const int ErrInvalidArg = 4;
    public const int ErrInternal = 5;
}

/// <summary>Exception thrown when a BCS FFI call fails.</summary>
public sealed class BcsException : Exception
{
    public int Code { get; }

    public BcsException(int code, string message) : base($"BCS error {code}: {message}")
    {
        Code = code;
    }
}

internal static class Native
{
    private static readonly IntPtr Handle;
    private static readonly string LibraryPath;

    static Native()
    {
        LibraryPath = ResolveLibraryPath();
        if (!NativeLibrary.TryLoad(LibraryPath, out Handle))
        {
            throw new FileNotFoundException(
                $"Could not load bcs_ffi from '{LibraryPath}'. " +
                "Build with `cargo build -p bcs-ffi --release` or set BCS_FFI_LIB.");
        }

        Version = Load<VersionDelegate>("bcs_version");
        LastError = Load<LastErrorDelegate>("bcs_last_error");
        EncodeJson = Load<EncodeJsonDelegate>("bcs_encode_json");
        DecodeToJson = Load<DecodeToJsonDelegate>("bcs_decode_to_json");
        DecodeToJsonEx = Load<DecodeToJsonExDelegate>("bcs_decode_to_json_ex");
        GetPathJson = Load<GetPathJsonDelegate>("bcs_get_path_json");
        SchemaExportJson = Load<SchemaExportJsonDelegate>("bcs_schema_export_json");
        Validate = Load<ValidateDelegate>("bcs_validate");
        ProtectJson = Load<ProtectJsonDelegate>("bcs_protect_json");
        ProtectJsonEx = Load<ProtectJsonExDelegate>("bcs_protect_json_ex");
        Strdup = Load<StrdupDelegate>("bcs_strdup");
        Alloc = Load<AllocDelegate>("bcs_alloc");
        FreeBuffer = Load<FreeBufferDelegate>("bcs_free_buffer");
        FreeString = Load<FreeStringDelegate>("bcs_free_string");
    }

    private static T Load<T>(string name) where T : Delegate
    {
        var ptr = NativeLibrary.GetExport(Handle, name);
        return Marshal.GetDelegateForFunctionPointer<T>(ptr);
    }

    private static string ResolveLibraryPath()
    {
        var env = Environment.GetEnvironmentVariable("BCS_FFI_LIB");
        if (!string.IsNullOrWhiteSpace(env) && File.Exists(env))
        {
            return env;
        }

        var root = FindRepoRoot();
        var (os, arch, fileName) = PlatformTriplet();
        var candidates = new List<string>
        {
            Path.Combine(root, "dist", "ffi", $"{os}-{arch}", fileName),
            Path.Combine(root, "target", "release", fileName),
            Path.Combine(root, "target", "debug", fileName),
        };

        var cargoTarget = Environment.GetEnvironmentVariable("CARGO_TARGET_DIR");
        if (!string.IsNullOrWhiteSpace(cargoTarget))
        {
            candidates.Add(Path.Combine(cargoTarget, "release", fileName));
            candidates.Add(Path.Combine(cargoTarget, "debug", fileName));
        }

        foreach (var path in candidates)
        {
            if (File.Exists(path))
            {
                return path;
            }
        }

        throw new FileNotFoundException(
            "Could not find bcs_ffi shared library. Tried:\n" +
            string.Join('\n', candidates));
    }

    private static string FindRepoRoot()
    {
        var dir = new DirectoryInfo(AppContext.BaseDirectory);
        while (dir != null)
        {
            if (File.Exists(Path.Combine(dir.FullName, "Cargo.toml")) &&
                Directory.Exists(Path.Combine(dir.FullName, "ffi")))
            {
                return dir.FullName;
            }
            dir = dir.Parent;
        }

        // Fallback: bindings/csharp/Bcs -> repo root is three levels up from project dir
        var asmDir = Path.GetDirectoryName(Assembly.GetExecutingAssembly().Location)
                     ?? Directory.GetCurrentDirectory();
        return Path.GetFullPath(Path.Combine(asmDir, "..", "..", "..", "..", ".."));
    }

    private static (string os, string arch, string fileName) PlatformTriplet()
    {
        var arch = RuntimeInformation.ProcessArchitecture switch
        {
            Architecture.Arm64 => "arm64",
            Architecture.X64 => "x64",
            _ => RuntimeInformation.ProcessArchitecture.ToString().ToLowerInvariant(),
        };

        if (OperatingSystem.IsMacOS())
        {
            return ("darwin", arch, "libbcs_ffi.dylib");
        }
        if (OperatingSystem.IsLinux())
        {
            return ("linux", arch, "libbcs_ffi.so");
        }
        if (OperatingSystem.IsWindows())
        {
            return ("windows", arch, "bcs_ffi.dll");
        }

        throw new PlatformNotSupportedException(RuntimeInformation.OSDescription);
    }

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    public delegate IntPtr VersionDelegate();

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    public delegate IntPtr LastErrorDelegate();

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    public delegate int EncodeJsonDelegate(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string json,
        int compact,
        int compressData,
        out IntPtr outPtr,
        out UIntPtr outLen);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    public delegate int DecodeToJsonDelegate(
        byte[] data,
        UIntPtr dataLen,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? password,
        out IntPtr outJson);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    public delegate int GetPathJsonDelegate(
        byte[] data,
        UIntPtr dataLen,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string path,
        out IntPtr outJson);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    public delegate int SchemaExportJsonDelegate(
        byte[] data,
        UIntPtr dataLen,
        out IntPtr outJson);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    public delegate int ValidateDelegate(
        byte[] data,
        UIntPtr dataLen,
        out int outOk);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    public delegate int ProtectJsonDelegate(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string json,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string pathsCsv,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string password,
        int compact,
        int compressData,
        out IntPtr outPtr,
        out UIntPtr outLen);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    public delegate int ProtectJsonExDelegate(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string json,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string pathsCsv,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? password,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? kmsProvider,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? kmsKey,
        IntPtr wrapFn,
        IntPtr wrapUserdata,
        int compact,
        int compressData,
        out IntPtr outPtr,
        out UIntPtr outLen);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    public delegate int DecodeToJsonExDelegate(
        byte[] data,
        UIntPtr dataLen,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? password,
        IntPtr resolveFn,
        IntPtr resolveUserdata,
        IntPtr unwrapFn,
        IntPtr unwrapUserdata,
        out IntPtr outJson);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    public delegate IntPtr SecretResolveDelegate(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string scheme,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string locator,
        IntPtr userdata);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    public delegate int KeyUnwrapDelegate(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string provider,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string kekLocator,
        IntPtr wrapped,
        UIntPtr wrappedLen,
        IntPtr outDek,
        IntPtr userdata);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    public delegate int KeyWrapDelegate(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string provider,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string kekLocator,
        IntPtr dek,
        UIntPtr dekLen,
        out IntPtr outWrapped,
        out UIntPtr outWrappedLen,
        IntPtr userdata);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    public delegate IntPtr StrdupDelegate(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string s);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    public delegate IntPtr AllocDelegate(UIntPtr len);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    public delegate void FreeBufferDelegate(IntPtr ptr, UIntPtr len);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    public delegate void FreeStringDelegate(IntPtr ptr);

    public static readonly VersionDelegate Version;
    public static readonly LastErrorDelegate LastError;
    public static readonly EncodeJsonDelegate EncodeJson;
    public static readonly DecodeToJsonDelegate DecodeToJson;
    public static readonly DecodeToJsonExDelegate DecodeToJsonEx;
    public static readonly GetPathJsonDelegate GetPathJson;
    public static readonly SchemaExportJsonDelegate SchemaExportJson;
    public static readonly ValidateDelegate Validate;
    public static readonly ProtectJsonDelegate ProtectJson;
    public static readonly ProtectJsonExDelegate ProtectJsonEx;
    public static readonly StrdupDelegate Strdup;
    public static readonly AllocDelegate Alloc;
    public static readonly FreeBufferDelegate FreeBuffer;
    public static readonly FreeStringDelegate FreeString;
}

/// <summary>High-level BCS API for .NET.</summary>
public static class BcsClient
{
    public static string Version()
    {
        var ptr = Native.Version();
        return ptr == IntPtr.Zero ? string.Empty : Marshal.PtrToStringUTF8(ptr) ?? string.Empty;
    }

    public static byte[] EncodeJson(string json, bool compact = false, bool compressData = false)
    {
        var code = Native.EncodeJson(json, compact ? 1 : 0, compressData ? 1 : 0, out var ptr, out var len);
        Check(code);
        try
        {
            var bytes = new byte[(int)len];
            Marshal.Copy(ptr, bytes, 0, bytes.Length);
            return bytes;
        }
        finally
        {
            Native.FreeBuffer(ptr, len);
        }
    }

    public static string DecodeToJson(byte[] data, string? password = null)
    {
        var code = Native.DecodeToJson(data, (UIntPtr)data.Length, password, out var ptr);
        Check(code);
        try
        {
            return Marshal.PtrToStringUTF8(ptr) ?? string.Empty;
        }
        finally
        {
            Native.FreeString(ptr);
        }
    }

    public delegate string? SecretResolveHandler(string scheme, string locator);
    public delegate byte[]? KeyUnwrapHandler(string provider, string kekLocator, byte[] wrapped);

    public static string DecodeToJsonEx(
        byte[] data,
        string? password = null,
        SecretResolveHandler? resolveSecrets = null,
        KeyUnwrapHandler? unwrapKey = null)
    {
        IntPtr resolveFn = IntPtr.Zero;
        IntPtr unwrapFn = IntPtr.Zero;
        Native.SecretResolveDelegate? resolveCallback = null;
        Native.KeyUnwrapDelegate? unwrapCallback = null;

        try
        {
            if (resolveSecrets != null)
            {
                SecretResolveHandler handler = resolveSecrets;
                resolveCallback = (string scheme, string locator, IntPtr ud) =>
                {
                    try
                    {
                        var result = handler(scheme, locator);
                        return result == null ? IntPtr.Zero : Native.Strdup(result);
                    }
                    catch
                    {
                        return IntPtr.Zero;
                    }
                };
                resolveFn = Marshal.GetFunctionPointerForDelegate(resolveCallback);
            }

            if (unwrapKey != null)
            {
                KeyUnwrapHandler handler = unwrapKey;
                unwrapCallback = (string provider, string kekLocator, IntPtr wrappedPtr, UIntPtr wrappedLen, IntPtr outDek, IntPtr ud) =>
                {
                    try
                    {
                        var wrapped = CopyNativeBytes(wrappedPtr, wrappedLen);
                        var result = handler(provider, kekLocator, wrapped);
                        if (result == null || result.Length != 32) return 1;
                        Marshal.Copy(result, 0, outDek, 32);
                        return 0;
                    }
                    catch
                    {
                        return 1;
                    }
                };
                unwrapFn = Marshal.GetFunctionPointerForDelegate(unwrapCallback);
            }

            var code = Native.DecodeToJsonEx(
                data, (UIntPtr)data.Length, password,
                resolveFn, IntPtr.Zero,
                unwrapFn, IntPtr.Zero,
                out var ptr);
            Check(code);
            try
            {
                return Marshal.PtrToStringUTF8(ptr) ?? string.Empty;
            }
            finally
            {
                Native.FreeString(ptr);
            }
        }
        finally
        {
            GC.KeepAlive(resolveCallback);
            GC.KeepAlive(unwrapCallback);
        }
    }

    public static string GetPathJson(byte[] data, string path)
    {
        var code = Native.GetPathJson(data, (UIntPtr)data.Length, path, out var ptr);
        Check(code);
        try
        {
            return Marshal.PtrToStringUTF8(ptr) ?? string.Empty;
        }
        finally
        {
            Native.FreeString(ptr);
        }
    }

    /// <summary>Export agent-safe schema JSON (paths/types/sensitive; never data-layer values).</summary>
    public static string SchemaExportJson(byte[] data)
    {
        var code = Native.SchemaExportJson(data, (UIntPtr)data.Length, out var ptr);
        Check(code);
        try
        {
            return Marshal.PtrToStringUTF8(ptr) ?? string.Empty;
        }
        finally
        {
            Native.FreeString(ptr);
        }
    }

    public static bool Validate(byte[] data)
    {
        var code = Native.Validate(data, (UIntPtr)data.Length, out var ok);
        Check(code);
        return ok == 1;
    }

    public static byte[] ProtectJson(
        string json,
        IEnumerable<string> paths,
        string password,
        bool compact = false,
        bool compressData = false)
    {
        var csv = string.Join(',', paths);
        var code = Native.ProtectJson(
            json,
            csv,
            password,
            compact ? 1 : 0,
            compressData ? 1 : 0,
            out var ptr,
            out var len);
        Check(code);
        try
        {
            var bytes = new byte[(int)len];
            Marshal.Copy(ptr, bytes, 0, bytes.Length);
            return bytes;
        }
        finally
        {
            Native.FreeBuffer(ptr, len);
        }
    }

    public delegate byte[]? KeyWrapHandler(string provider, string kekLocator, byte[] dek);

    public static byte[] ProtectJsonEx(
        string json,
        IEnumerable<string> paths,
        string? password = null,
        string? kmsProvider = null,
        string? kmsKey = null,
        KeyWrapHandler? wrapKey = null,
        bool compact = false,
        bool compressData = false)
    {
        var csv = string.Join(',', paths);
        IntPtr wrapFn = IntPtr.Zero;
        Native.KeyWrapDelegate? wrapCallback = null;

        try
        {
            if (wrapKey != null && kmsProvider != null && kmsKey != null)
            {
                KeyWrapHandler handler = wrapKey;
                wrapCallback = (string provider, string kekLocator, IntPtr dekPtr, UIntPtr dekLen, out IntPtr outWrapped, out UIntPtr outWrappedLen, IntPtr ud) =>
                {
                    try
                    {
                        var dek = CopyNativeBytes(dekPtr, dekLen);
                        var result = handler(provider, kekLocator, dek);
                        if (result == null)
                        {
                            outWrapped = IntPtr.Zero;
                            outWrappedLen = UIntPtr.Zero;
                            return 1;
                        }
                        var buf = Native.Alloc((UIntPtr)result.Length);
                        Marshal.Copy(result, 0, buf, result.Length);
                        outWrapped = buf;
                        outWrappedLen = (UIntPtr)result.Length;
                        return 0;
                    }
                    catch
                    {
                        outWrapped = IntPtr.Zero;
                        outWrappedLen = UIntPtr.Zero;
                        return 1;
                    }
                };
                wrapFn = Marshal.GetFunctionPointerForDelegate(wrapCallback);
            }

            var code = Native.ProtectJsonEx(
                json, csv, password, kmsProvider, kmsKey,
                wrapFn, IntPtr.Zero,
                compact ? 1 : 0, compressData ? 1 : 0,
                out var ptr, out var len);
            Check(code);
            try
            {
                var bytes = new byte[(int)len];
                Marshal.Copy(ptr, bytes, 0, bytes.Length);
                return bytes;
            }
            finally
            {
                Native.FreeBuffer(ptr, len);
            }
        }
        finally
        {
            GC.KeepAlive(wrapCallback);
        }
    }

    private static byte[] CopyNativeBytes(IntPtr ptr, UIntPtr length)
    {
        var length64 = length.ToUInt64();
        if (length64 > int.MaxValue)
        {
            throw new ArgumentOutOfRangeException(nameof(length), "Native buffer is too large.");
        }

        var bytes = new byte[(int)length64];
        if (bytes.Length > 0)
        {
            Marshal.Copy(ptr, bytes, 0, bytes.Length);
        }
        return bytes;
    }

    private static void Check(int code)
    {
        if (code == BcsStatus.Ok)
        {
            return;
        }

        var errPtr = Native.LastError();
        var message = errPtr == IntPtr.Zero
            ? "unknown error"
            : Marshal.PtrToStringUTF8(errPtr) ?? "unknown error";
        throw new BcsException(code, message);
    }
}

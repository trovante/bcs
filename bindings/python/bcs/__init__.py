"""Python bindings for Binary Config Schema (BCS) via the C FFI."""

from __future__ import annotations

import ctypes
import os
import platform
import sys
from pathlib import Path
from typing import Callable, Optional


BCS_OK = 0
BCS_ERR_NULL = 1
BCS_ERR_UTF8 = 2
BCS_ERR_FORMAT = 3
BCS_ERR_INVALID_ARG = 4
BCS_ERR_INTERNAL = 5


class BCSError(RuntimeError):
    def __init__(self, code: int, message: str):
        super().__init__(f"BCS error {code}: {message}")
        self.code = code
        self.message = message


def _repo_root() -> Path:
    # bindings/python/bcs/__init__.py -> repo root is four levels up
    return Path(__file__).resolve().parents[3]


def _candidate_lib_paths() -> list[Path]:
    root = _repo_root()
    system = platform.system().lower()
    machine = platform.machine().lower()
    arch = "arm64" if machine in ("arm64", "aarch64") else "x64"
    os_name = "darwin" if system == "darwin" else "linux" if system == "linux" else system

    names: list[str]
    if system == "darwin":
        names = ["libbcs_ffi.dylib"]
    elif system == "windows":
        names = ["bcs_ffi.dll", "libbcs_ffi.dll"]
    else:
        names = ["libbcs_ffi.so"]

    cargo_target = Path(os.environ["CARGO_TARGET_DIR"]) if os.environ.get("CARGO_TARGET_DIR") else None
    bases = [
        root / "dist" / "ffi" / f"{os_name}-{arch}",
        root / "target" / "release",
        root / "target" / "debug",
    ]
    if cargo_target is not None:
        bases.extend([cargo_target / "release", cargo_target / "debug"])

    paths: list[Path] = []
    env_lib = os.environ.get("BCS_FFI_LIB")
    if env_lib:
        paths.append(Path(env_lib))
    for base in bases:
        for name in names:
            paths.append(base / name)
    return paths


def _load_library() -> ctypes.CDLL:
    last_err: Optional[OSError] = None
    for path in _candidate_lib_paths():
        if not path.exists():
            continue
        try:
            return ctypes.CDLL(str(path))
        except OSError as exc:
            last_err = exc
    raise FileNotFoundError(
        "Could not load bcs_ffi shared library. Build with "
        "`cargo build -p bcs-ffi --release` or set BCS_FFI_LIB. "
        f"Last error: {last_err}"
    )


class _Lib:
    def __init__(self) -> None:
        self.lib = _load_library()
        self._configure()

    def _configure(self) -> None:
        lib = self.lib

        lib.bcs_version.restype = ctypes.c_char_p
        lib.bcs_last_error.restype = ctypes.c_char_p

        lib.bcs_encode_json.argtypes = [
            ctypes.c_char_p,
            ctypes.c_int,
            ctypes.c_int,
            ctypes.POINTER(ctypes.POINTER(ctypes.c_uint8)),
            ctypes.POINTER(ctypes.c_size_t),
        ]
        lib.bcs_encode_json.restype = ctypes.c_int

        lib.bcs_decode_to_json.argtypes = [
            ctypes.POINTER(ctypes.c_uint8),
            ctypes.c_size_t,
            ctypes.c_char_p,
            ctypes.POINTER(ctypes.c_char_p),
        ]
        lib.bcs_decode_to_json.restype = ctypes.c_int

        self.SecretResolveFn = ctypes.CFUNCTYPE(
            ctypes.c_void_p,
            ctypes.c_char_p,
            ctypes.c_char_p,
            ctypes.c_void_p,
        )
        self.KeyWrapFn = ctypes.CFUNCTYPE(
            ctypes.c_int,
            ctypes.c_char_p,
            ctypes.c_char_p,
            ctypes.POINTER(ctypes.c_uint8),
            ctypes.c_size_t,
            ctypes.POINTER(ctypes.POINTER(ctypes.c_uint8)),
            ctypes.POINTER(ctypes.c_size_t),
            ctypes.c_void_p,
        )
        self.KeyUnwrapFn = ctypes.CFUNCTYPE(
            ctypes.c_int,
            ctypes.c_char_p,
            ctypes.c_char_p,
            ctypes.POINTER(ctypes.c_uint8),
            ctypes.c_size_t,
            ctypes.POINTER(ctypes.c_uint8),
            ctypes.c_void_p,
        )
        lib.bcs_decode_to_json_ex.argtypes = [
            ctypes.POINTER(ctypes.c_uint8),
            ctypes.c_size_t,
            ctypes.c_char_p,
            ctypes.c_void_p,  # resolve_fn (optional)
            ctypes.c_void_p,
            ctypes.c_void_p,  # unwrap_fn (optional)
            ctypes.c_void_p,
            ctypes.POINTER(ctypes.c_char_p),
        ]
        lib.bcs_decode_to_json_ex.restype = ctypes.c_int

        # c_void_p avoids ctypes auto-converting callback returns to Python bytes.
        lib.bcs_strdup.argtypes = [ctypes.c_char_p]
        lib.bcs_strdup.restype = ctypes.c_void_p

        lib.bcs_alloc.argtypes = [ctypes.c_size_t]
        lib.bcs_alloc.restype = ctypes.POINTER(ctypes.c_uint8)

        lib.bcs_get_path_json.argtypes = [
            ctypes.POINTER(ctypes.c_uint8),
            ctypes.c_size_t,
            ctypes.c_char_p,
            ctypes.POINTER(ctypes.c_char_p),
        ]
        lib.bcs_get_path_json.restype = ctypes.c_int

        lib.bcs_schema_export_json.argtypes = [
            ctypes.POINTER(ctypes.c_uint8),
            ctypes.c_size_t,
            ctypes.POINTER(ctypes.c_char_p),
        ]
        lib.bcs_schema_export_json.restype = ctypes.c_int

        lib.bcs_validate.argtypes = [
            ctypes.POINTER(ctypes.c_uint8),
            ctypes.c_size_t,
            ctypes.POINTER(ctypes.c_int),
        ]
        lib.bcs_validate.restype = ctypes.c_int

        lib.bcs_protect_json.argtypes = [
            ctypes.c_char_p,
            ctypes.c_char_p,
            ctypes.c_char_p,
            ctypes.c_int,
            ctypes.c_int,
            ctypes.POINTER(ctypes.POINTER(ctypes.c_uint8)),
            ctypes.POINTER(ctypes.c_size_t),
        ]
        lib.bcs_protect_json.restype = ctypes.c_int

        lib.bcs_protect_json_ex.argtypes = [
            ctypes.c_char_p,
            ctypes.c_char_p,
            ctypes.c_char_p,
            ctypes.c_char_p,
            ctypes.c_char_p,
            ctypes.c_void_p,  # wrap_fn (optional)
            ctypes.c_void_p,
            ctypes.c_int,
            ctypes.c_int,
            ctypes.POINTER(ctypes.POINTER(ctypes.c_uint8)),
            ctypes.POINTER(ctypes.c_size_t),
        ]
        lib.bcs_protect_json_ex.restype = ctypes.c_int

        lib.bcs_free_buffer.argtypes = [ctypes.POINTER(ctypes.c_uint8), ctypes.c_size_t]
        lib.bcs_free_buffer.restype = None
        lib.bcs_free_string.argtypes = [ctypes.c_char_p]
        lib.bcs_free_string.restype = None

    def last_error(self) -> str:
        ptr = self.lib.bcs_last_error()
        if not ptr:
            return "unknown error"
        return ptr.decode("utf-8", errors="replace")

    def check(self, code: int) -> None:
        if code != BCS_OK:
            raise BCSError(code, self.last_error())


_LIB: Optional[_Lib] = None


def _lib() -> _Lib:
    global _LIB
    if _LIB is None:
        _LIB = _Lib()
    return _LIB


def version() -> str:
    raw = _lib().lib.bcs_version()
    return raw.decode("utf-8") if raw else ""


def encode_json(json_text: str, *, compact: bool = False, compress_data: bool = False) -> bytes:
    lib = _lib()
    out_ptr = ctypes.POINTER(ctypes.c_uint8)()
    out_len = ctypes.c_size_t(0)
    code = lib.lib.bcs_encode_json(
        json_text.encode("utf-8"),
        1 if compact else 0,
        1 if compress_data else 0,
        ctypes.byref(out_ptr),
        ctypes.byref(out_len),
    )
    lib.check(code)
    try:
        return ctypes.string_at(out_ptr, out_len.value)
    finally:
        lib.lib.bcs_free_buffer(out_ptr, out_len)


ResolveSecrets = Callable[[str, str], Optional[str]]
WrapKey = Callable[[str, str, bytes], bytes]
UnwrapKey = Callable[[str, str, bytes], bytes]


def decode_to_json(
    data: bytes,
    password: Optional[str] = None,
    *,
    resolve_secrets: Optional[ResolveSecrets] = None,
    unwrap_key: Optional[UnwrapKey] = None,
) -> str:
    """Decode BCS bytes to JSON.

    When ``resolve_secrets`` is provided it is called as ``fn(scheme, locator)``
    for each ``__bcs_secret_ref__:`` marker. Return the plaintext string, or
    ``None`` / raise to fail the decode. When omitted, refs are masked.

    When ``unwrap_key`` is provided it is called as
    ``fn(provider, kek_locator, wrapped_dek) -> 32-byte DEK`` for ``kms`` markers.
    """
    lib = _lib()
    buf = (ctypes.c_uint8 * len(data)).from_buffer_copy(data)
    out = ctypes.c_char_p()
    pwd = password.encode("utf-8") if password is not None else None

    if resolve_secrets is None and unwrap_key is None:
        code = lib.lib.bcs_decode_to_json(buf, len(data), pwd, ctypes.byref(out))
    else:
        resolve_cb = None
        unwrap_cb = None

        if resolve_secrets is not None:
            def _resolve_trampoline(scheme_p, locator_p, _userdata):
                scheme = scheme_p.decode("utf-8") if scheme_p else ""
                locator = locator_p.decode("utf-8") if locator_p else ""
                try:
                    value = resolve_secrets(scheme, locator)
                except Exception:
                    return None
                if value is None:
                    return None
                return lib.lib.bcs_strdup(value.encode("utf-8"))

            resolve_cb = lib.SecretResolveFn(_resolve_trampoline)

        if unwrap_key is not None:
            def _unwrap_trampoline(provider_p, kek_p, wrapped_p, wrapped_len, out_dek, _userdata):
                provider = provider_p.decode("utf-8") if provider_p else ""
                kek = kek_p.decode("utf-8") if kek_p else ""
                wrapped = ctypes.string_at(wrapped_p, wrapped_len)
                try:
                    dek = unwrap_key(provider, kek, wrapped)
                except Exception:
                    return BCS_ERR_FORMAT
                if not isinstance(dek, (bytes, bytearray)) or len(dek) != 32:
                    return BCS_ERR_FORMAT
                ctypes.memmove(out_dek, bytes(dek), 32)
                return BCS_OK

            unwrap_cb = lib.KeyUnwrapFn(_unwrap_trampoline)

        code = lib.lib.bcs_decode_to_json_ex(
            buf,
            len(data),
            pwd,
            ctypes.cast(resolve_cb, ctypes.c_void_p) if resolve_cb is not None else None,
            None,
            ctypes.cast(unwrap_cb, ctypes.c_void_p) if unwrap_cb is not None else None,
            None,
            ctypes.byref(out),
        )

    lib.check(code)
    try:
        return out.value.decode("utf-8") if out.value else ""
    finally:
        lib.lib.bcs_free_string(out)


def get_path_json(data: bytes, path: str) -> str:
    lib = _lib()
    buf = (ctypes.c_uint8 * len(data)).from_buffer_copy(data)
    out = ctypes.c_char_p()
    code = lib.lib.bcs_get_path_json(buf, len(data), path.encode("utf-8"), ctypes.byref(out))
    lib.check(code)
    try:
        return out.value.decode("utf-8") if out.value else ""
    finally:
        lib.lib.bcs_free_string(out)


def schema_export_json(data: bytes) -> str:
    """Export agent-safe schema JSON (paths/types/sensitive; never data values)."""
    lib = _lib()
    buf = (ctypes.c_uint8 * len(data)).from_buffer_copy(data)
    out = ctypes.c_char_p()
    code = lib.lib.bcs_schema_export_json(buf, len(data), ctypes.byref(out))
    lib.check(code)
    try:
        return out.value.decode("utf-8") if out.value else ""
    finally:
        lib.lib.bcs_free_string(out)


def validate(data: bytes) -> bool:
    lib = _lib()
    buf = (ctypes.c_uint8 * len(data)).from_buffer_copy(data)
    ok = ctypes.c_int(0)
    code = lib.lib.bcs_validate(buf, len(data), ctypes.byref(ok))
    lib.check(code)
    return ok.value == 1


def protect_json(
    json_text: str,
    paths: list[str],
    password: Optional[str] = None,
    *,
    kms_provider: Optional[str] = None,
    kms_key: Optional[str] = None,
    wrap_key: Optional[WrapKey] = None,
    compact: bool = False,
    compress_data: bool = False,
) -> bytes:
    """Protect paths with ``pbkdf2`` (password) or ``kms`` (wrap_key callback)."""
    lib = _lib()
    out_ptr = ctypes.POINTER(ctypes.c_uint8)()
    out_len = ctypes.c_size_t(0)

    if password is not None:
        code = lib.lib.bcs_protect_json(
            json_text.encode("utf-8"),
            ",".join(paths).encode("utf-8"),
            password.encode("utf-8"),
            1 if compact else 0,
            1 if compress_data else 0,
            ctypes.byref(out_ptr),
            ctypes.byref(out_len),
        )
    else:
        if not kms_provider or not kms_key or wrap_key is None:
            raise ValueError("kms protect requires kms_provider, kms_key, and wrap_key")

        def _wrap_trampoline(provider_p, kek_p, dek_p, dek_len, out_wrapped, out_len_p, _userdata):
            provider = provider_p.decode("utf-8") if provider_p else ""
            kek = kek_p.decode("utf-8") if kek_p else ""
            dek = ctypes.string_at(dek_p, dek_len)
            try:
                wrapped = wrap_key(provider, kek, dek)
            except Exception:
                return BCS_ERR_FORMAT
            if not isinstance(wrapped, (bytes, bytearray)) or len(wrapped) == 0:
                return BCS_ERR_FORMAT
            buf = lib.lib.bcs_alloc(len(wrapped))
            if not buf:
                return BCS_ERR_INTERNAL
            ctypes.memmove(buf, bytes(wrapped), len(wrapped))
            out_wrapped[0] = buf
            out_len_p[0] = len(wrapped)
            return BCS_OK

        wrap_cb = lib.KeyWrapFn(_wrap_trampoline)
        code = lib.lib.bcs_protect_json_ex(
            json_text.encode("utf-8"),
            ",".join(paths).encode("utf-8"),
            None,
            kms_provider.encode("utf-8"),
            kms_key.encode("utf-8"),
            ctypes.cast(wrap_cb, ctypes.c_void_p),
            None,
            1 if compact else 0,
            1 if compress_data else 0,
            ctypes.byref(out_ptr),
            ctypes.byref(out_len),
        )

    lib.check(code)
    try:
        return ctypes.string_at(out_ptr, out_len.value)
    finally:
        lib.lib.bcs_free_buffer(out_ptr, out_len)


def _self_test() -> None:
    data = encode_json('{"server":{"host":"localhost"},"database":{"password":"secret"}}')
    assert validate(data)
    assert '"localhost"' in get_path_json(data, "server.host")
    schema = schema_export_json(data)
    assert "password" in schema or "database" in schema
    assert "secret" not in schema
    protected = protect_json(
        '{"database":{"password":"secret"}}',
        ["database.password"],
        password="master",
    )
    masked = decode_to_json(protected)
    assert "[PROTECTED]" in masked
    revealed = decode_to_json(protected, password="master")
    assert "secret" in revealed

    marker = "__bcs_secret_ref__:env:BCS_PY_TEST_TOKEN"
    os.environ["BCS_PY_TEST_TOKEN"] = "tok_from_python"
    ref_data = encode_json(f'{{"api":{{"token":"{marker}"}}}}')
    assert "[SECRET_REF]" in decode_to_json(ref_data)
    resolved = decode_to_json(
        ref_data,
        resolve_secrets=lambda scheme, locator: os.environ.get(locator)
        if scheme == "env"
        else None,
    )
    assert "tok_from_python" in resolved
    del os.environ["BCS_PY_TEST_TOKEN"]

    def xor_wrap(_provider: str, _kek: str, dek: bytes) -> bytes:
        return bytes(b ^ 0xA5 for b in dek)

    def xor_unwrap(_provider: str, _kek: str, wrapped: bytes) -> bytes:
        return bytes(b ^ 0xA5 for b in wrapped)

    kms_protected = protect_json(
        '{"database":{"password":"kms-secret"}}',
        ["database.password"],
        kms_provider="cmd",
        kms_key="alias/test",
        wrap_key=xor_wrap,
    )
    assert "[PROTECTED]" in decode_to_json(kms_protected)
    kms_revealed = decode_to_json(kms_protected, unwrap_key=xor_unwrap)
    assert "kms-secret" in kms_revealed

    print(f"bcs python bindings ok (version={version()})")


if __name__ == "__main__":
    _self_test()

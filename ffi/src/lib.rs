//! C FFI surface for embedding BCS in other languages.
//!
//! Build:
//! ```bash
//! cargo build -p bcs-ffi --release
//! ```
//!
//! Consumers should include `ffi/include/bcs.h` and link `bcs_ffi`.

use bcs_core::convert::value_to_json;
use bcs_core::secret_resolver::SecretResolver;
use bcs_core::security::KeyWrapper;
use bcs_core::types::Value;
use bcs_core::{security, Decoder, Encoder, EncoderConfig};
use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;
use std::slice;

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

/// Success.
pub const BCS_OK: c_int = 0;
/// A required pointer argument was null.
pub const BCS_ERR_NULL: c_int = 1;
/// Input was not valid UTF-8.
pub const BCS_ERR_UTF8: c_int = 2;
/// Encode/decode/format failure (see [`bcs_last_error`]).
pub const BCS_ERR_FORMAT: c_int = 3;
/// Invalid argument combination or empty required value.
pub const BCS_ERR_INVALID_ARG: c_int = 4;
/// Internal / allocation failure.
pub const BCS_ERR_INTERNAL: c_int = 5;

fn set_last_error(msg: impl Into<String>) {
    let msg = msg.into().replace('\0', "");
    let cstr = CString::new(msg).unwrap_or_else(|_| {
        CString::new("failed to construct error message").expect("static ASCII")
    });
    LAST_ERROR.with(|slot| *slot.borrow_mut() = Some(cstr));
}

fn clear_last_error() {
    LAST_ERROR.with(|slot| *slot.borrow_mut() = None);
}

fn cstr_to_str<'a>(ptr: *const c_char) -> Result<&'a str, c_int> {
    if ptr.is_null() {
        set_last_error("null string pointer");
        return Err(BCS_ERR_NULL);
    }
    // Safety: caller guarantees a valid NUL-terminated C string.
    let s = unsafe { CStr::from_ptr(ptr) };
    s.to_str().map_err(|_| {
        set_last_error("input is not valid UTF-8");
        BCS_ERR_UTF8
    })
}

fn alloc_bytes(bytes: Vec<u8>, out_ptr: *mut *mut u8, out_len: *mut usize) -> c_int {
    if out_ptr.is_null() || out_len.is_null() {
        set_last_error("null output pointer");
        return BCS_ERR_NULL;
    }
    let len = bytes.len();
    let mut boxed = bytes.into_boxed_slice();
    let ptr = boxed.as_mut_ptr();
    std::mem::forget(boxed);
    unsafe {
        *out_ptr = ptr;
        *out_len = len;
    }
    BCS_OK
}

fn alloc_cstring(s: String, out: *mut *mut c_char) -> c_int {
    if out.is_null() {
        set_last_error("null output string pointer");
        return BCS_ERR_NULL;
    }
    match CString::new(s) {
        Ok(cstr) => {
            unsafe {
                *out = cstr.into_raw();
            }
            BCS_OK
        }
        Err(_) => {
            set_last_error("output contained interior NUL");
            BCS_ERR_INTERNAL
        }
    }
}

fn encode_with_config(json: &str, config: EncoderConfig) -> Result<Vec<u8>, c_int> {
    let mut encoder = Encoder::with_config(config);
    encoder.encode_from_json(json).map_err(|e| {
        set_last_error(e.to_string());
        BCS_ERR_FORMAT
    })
}

/// Return the last error message for this thread (static until next failing call).
///
/// # Safety
/// Pointer is valid until the next FFI call on this thread that sets an error.
#[no_mangle]
pub unsafe extern "C" fn bcs_last_error() -> *const c_char {
    LAST_ERROR.with(|slot| match slot.borrow().as_ref() {
        Some(s) => s.as_ptr(),
        None => ptr::null(),
    })
}

/// Library version string (`"0.1.0"`).
#[no_mangle]
pub extern "C" fn bcs_version() -> *const c_char {
    static VERSION: &str = "0.1.0\0";
    VERSION.as_ptr() as *const c_char
}

/// Encode JSON text to BCS bytes.
///
/// Flags:
/// - `compact != 0`: omit schema/index
/// - `compress_data != 0`: enable data-layer compression
///
/// # Safety
/// - `json_ptr` must be a valid NUL-terminated UTF-8 C string.
/// - `out_ptr` / `out_len` must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn bcs_encode_json(
    json_ptr: *const c_char,
    compact: c_int,
    compress_data: c_int,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> c_int {
    clear_last_error();
    let json = match cstr_to_str(json_ptr) {
        Ok(s) => s,
        Err(code) => return code,
    };

    let mut config = EncoderConfig::default();
    if compact != 0 {
        config.include_semantic_layer = false;
        config.include_index_table = false;
        config.compression = false;
    }
    config.data_compression = compress_data != 0;

    match encode_with_config(json, config) {
        Ok(bytes) => alloc_bytes(bytes, out_ptr, out_len),
        Err(code) => code,
    }
}

/// Decode BCS bytes to a newly allocated JSON string.
///
/// Optional `password_ptr` may be null. When null, protected fields stay masked.
/// Secret references are always masked (see [`bcs_decode_to_json_ex`] to resolve).
///
/// # Safety
/// - `data_ptr` must point to `data_len` readable bytes.
/// - `out_json` must be a valid writable pointer.
/// - `password_ptr` may be null or a valid NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn bcs_decode_to_json(
    data_ptr: *const u8,
    data_len: usize,
    password_ptr: *const c_char,
    out_json: *mut *mut c_char,
) -> c_int {
    bcs_decode_to_json_ex(
        data_ptr,
        data_len,
        password_ptr,
        None,
        ptr::null_mut(),
        None,
        ptr::null_mut(),
        out_json,
    )
}

/// C callback type for resolving secret references.
///
/// Returns a string allocated with [`bcs_strdup`] (freed by BCS via
/// [`bcs_free_string`]), or null on failure.
pub type BcsSecretResolveFn = Option<
    unsafe extern "C" fn(
        scheme: *const c_char,
        locator: *const c_char,
        userdata: *mut c_void,
    ) -> *mut c_char,
>;

/// C callback type for wrapping a DEK (`kms` protect scheme).
pub type BcsKeyWrapFn = Option<
    unsafe extern "C" fn(
        provider: *const c_char,
        kek_locator: *const c_char,
        dek: *const u8,
        dek_len: usize,
        out_wrapped: *mut *mut u8,
        out_wrapped_len: *mut usize,
        userdata: *mut c_void,
    ) -> c_int,
>;

/// C callback type for unwrapping a DEK (`kms` reveal).
pub type BcsKeyUnwrapFn = Option<
    unsafe extern "C" fn(
        provider: *const c_char,
        kek_locator: *const c_char,
        wrapped: *const u8,
        wrapped_len: usize,
        out_dek: *mut u8,
        userdata: *mut c_void,
    ) -> c_int,
>;

struct FfiCallbackResolver {
    resolve_fn: unsafe extern "C" fn(*const c_char, *const c_char, *mut c_void) -> *mut c_char,
    userdata: *mut c_void,
}

impl SecretResolver for FfiCallbackResolver {
    fn resolve(&self, scheme: &str, locator: &str) -> bcs_core::Result<String> {
        let scheme_c = CString::new(scheme).map_err(|_| {
            bcs_core::BCSError::Decoding("secret scheme contained interior NUL".to_string())
        })?;
        let locator_c = CString::new(locator).map_err(|_| {
            bcs_core::BCSError::Decoding("secret locator contained interior NUL".to_string())
        })?;

        let ptr =
            unsafe { (self.resolve_fn)(scheme_c.as_ptr(), locator_c.as_ptr(), self.userdata) };
        if ptr.is_null() {
            return Err(bcs_core::BCSError::Decoding(format!(
                "secret resolve callback returned null for '{}:{}'",
                scheme, locator
            )));
        }

        let owned = unsafe { CStr::from_ptr(ptr) }
            .to_str()
            .map(|s| s.to_string())
            .map_err(|_| {
                bcs_core::BCSError::Decoding(
                    "secret resolve callback returned non-UTF-8 data".to_string(),
                )
            });
        unsafe {
            bcs_free_string(ptr);
        }
        owned
    }
}

struct FfiCallbackKeyWrapper {
    wrap_fn: Option<
        unsafe extern "C" fn(
            *const c_char,
            *const c_char,
            *const u8,
            usize,
            *mut *mut u8,
            *mut usize,
            *mut c_void,
        ) -> c_int,
    >,
    unwrap_fn: Option<
        unsafe extern "C" fn(
            *const c_char,
            *const c_char,
            *const u8,
            usize,
            *mut u8,
            *mut c_void,
        ) -> c_int,
    >,
    userdata: *mut c_void,
}

impl KeyWrapper for FfiCallbackKeyWrapper {
    fn wrap(&self, provider: &str, kek_locator: &str, dek: &[u8]) -> bcs_core::Result<Vec<u8>> {
        let wrap_fn = self.wrap_fn.ok_or_else(|| {
            bcs_core::BCSError::Encoding("KMS wrap callback is not set".to_string())
        })?;
        let provider_c = CString::new(provider).map_err(|_| {
            bcs_core::BCSError::Encoding("KMS provider contained interior NUL".to_string())
        })?;
        let kek_c = CString::new(kek_locator).map_err(|_| {
            bcs_core::BCSError::Encoding("KMS key locator contained interior NUL".to_string())
        })?;
        let mut out_ptr: *mut u8 = ptr::null_mut();
        let mut out_len: usize = 0;
        let rc = unsafe {
            wrap_fn(
                provider_c.as_ptr(),
                kek_c.as_ptr(),
                dek.as_ptr(),
                dek.len(),
                &mut out_ptr,
                &mut out_len,
                self.userdata,
            )
        };
        if rc != BCS_OK || out_ptr.is_null() || out_len == 0 {
            return Err(bcs_core::BCSError::Encoding(
                "KMS wrap callback failed".to_string(),
            ));
        }
        let wrapped = unsafe { slice::from_raw_parts(out_ptr, out_len).to_vec() };
        unsafe {
            bcs_free_buffer(out_ptr, out_len);
        }
        Ok(wrapped)
    }

    fn unwrap(
        &self,
        provider: &str,
        kek_locator: &str,
        wrapped_dek: &[u8],
    ) -> bcs_core::Result<[u8; 32]> {
        let unwrap_fn = self.unwrap_fn.ok_or_else(|| {
            bcs_core::BCSError::Decoding("KMS unwrap callback is not set".to_string())
        })?;
        let provider_c = CString::new(provider).map_err(|_| {
            bcs_core::BCSError::Decoding("KMS provider contained interior NUL".to_string())
        })?;
        let kek_c = CString::new(kek_locator).map_err(|_| {
            bcs_core::BCSError::Decoding("KMS key locator contained interior NUL".to_string())
        })?;
        let mut out_dek = [0u8; 32];
        let rc = unsafe {
            unwrap_fn(
                provider_c.as_ptr(),
                kek_c.as_ptr(),
                wrapped_dek.as_ptr(),
                wrapped_dek.len(),
                out_dek.as_mut_ptr(),
                self.userdata,
            )
        };
        if rc != BCS_OK {
            return Err(bcs_core::BCSError::Decoding(
                "KMS unwrap callback failed".to_string(),
            ));
        }
        Ok(out_dek)
    }
}

/// Decode with optional password reveal, secret-ref resolve, and KMS unwrap.
///
/// # Safety
/// Same pointer rules as [`bcs_decode_to_json`]. Userdata pointers must remain
/// valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn bcs_decode_to_json_ex(
    data_ptr: *const u8,
    data_len: usize,
    password_ptr: *const c_char,
    resolve_fn: BcsSecretResolveFn,
    resolve_userdata: *mut c_void,
    unwrap_fn: BcsKeyUnwrapFn,
    unwrap_userdata: *mut c_void,
    out_json: *mut *mut c_char,
) -> c_int {
    clear_last_error();
    if data_ptr.is_null() {
        set_last_error("null data pointer");
        return BCS_ERR_NULL;
    }

    let data = slice::from_raw_parts(data_ptr, data_len);
    let mut decoder = match Decoder::from_bytes(data) {
        Ok(d) => d,
        Err(e) => {
            set_last_error(e.to_string());
            return BCS_ERR_FORMAT;
        }
    };

    let mut value = match decoder.decode_to_value() {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e.to_string());
            return BCS_ERR_FORMAT;
        }
    };

    if let Err(code) = apply_protection_and_secrets(
        &mut value,
        password_ptr,
        resolve_fn,
        resolve_userdata,
        unwrap_fn,
        unwrap_userdata,
    ) {
        return code;
    }

    let json_value = match value_to_json(&value) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e.to_string());
            return BCS_ERR_FORMAT;
        }
    };
    let json = match serde_json::to_string_pretty(&json_value) {
        Ok(s) => s,
        Err(e) => {
            set_last_error(e.to_string());
            return BCS_ERR_FORMAT;
        }
    };
    alloc_cstring(json, out_json)
}

fn apply_protection_and_secrets(
    value: &mut Value,
    password_ptr: *const c_char,
    resolve_fn: BcsSecretResolveFn,
    resolve_userdata: *mut c_void,
    unwrap_fn: BcsKeyUnwrapFn,
    unwrap_userdata: *mut c_void,
) -> Result<(), c_int> {
    let password = if password_ptr.is_null() {
        None
    } else {
        Some(cstr_to_str(password_ptr)?)
    };

    let key_wrapper = unwrap_fn.map(|f| FfiCallbackKeyWrapper {
        wrap_fn: None,
        unwrap_fn: Some(f),
        userdata: unwrap_userdata,
    });

    if password.is_some() || key_wrapper.is_some() {
        let wrapper_ref = key_wrapper.as_ref().map(|w| w as &dyn KeyWrapper);
        if let Err(e) = security::reveal_all_ex(value, password, wrapper_ref) {
            set_last_error(e.to_string());
            return Err(BCS_ERR_FORMAT);
        }
    } else if let Err(e) = security::mask_all(value) {
        set_last_error(e.to_string());
        return Err(BCS_ERR_FORMAT);
    }

    if let Some(resolve_fn) = resolve_fn {
        let resolver = FfiCallbackResolver {
            resolve_fn,
            userdata: resolve_userdata,
        };
        if let Err(e) = security::resolve_secret_refs(value, &resolver) {
            set_last_error(e.to_string());
            return Err(BCS_ERR_FORMAT);
        }
    } else if let Err(e) = security::mask_secret_refs(value) {
        set_last_error(e.to_string());
        return Err(BCS_ERR_FORMAT);
    }
    Ok(())
}

/// Allocate a NUL-terminated copy of `s` for host callback return values.
///
/// # Safety
/// `s` must be null or a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn bcs_strdup(s: *const c_char) -> *mut c_char {
    if s.is_null() {
        return ptr::null_mut();
    }
    match CStr::from_ptr(s).to_str() {
        Ok(text) => match CString::new(text) {
            Ok(cstr) => cstr.into_raw(),
            Err(_) => ptr::null_mut(),
        },
        Err(_) => ptr::null_mut(),
    }
}

/// Allocate `len` bytes for FFI callback outputs (e.g. wrapped DEK).
#[no_mangle]
pub extern "C" fn bcs_alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return ptr::null_mut();
    }
    let mut buf = vec![0u8; len];
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// Decode a path query from BCS bytes into a JSON string.
///
/// # Safety
/// Same pointer rules as [`bcs_decode_to_json`], plus `path_ptr` must be valid UTF-8.
#[no_mangle]
pub unsafe extern "C" fn bcs_get_path_json(
    data_ptr: *const u8,
    data_len: usize,
    path_ptr: *const c_char,
    out_json: *mut *mut c_char,
) -> c_int {
    clear_last_error();
    if data_ptr.is_null() {
        set_last_error("null data pointer");
        return BCS_ERR_NULL;
    }
    let path = match cstr_to_str(path_ptr) {
        Ok(s) => s,
        Err(code) => return code,
    };
    if path.is_empty() {
        set_last_error("path cannot be empty");
        return BCS_ERR_INVALID_ARG;
    }

    let data = slice::from_raw_parts(data_ptr, data_len);
    let mut decoder = match Decoder::from_bytes(data) {
        Ok(d) => d,
        Err(e) => {
            set_last_error(e.to_string());
            return BCS_ERR_FORMAT;
        }
    };

    let mut value = match decoder.get(path) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e.to_string());
            return BCS_ERR_FORMAT;
        }
    };

    // Path queries default to masked sensitive content (no password / resolve).
    if security::is_protected_marker(&value) {
        value = Value::String("[PROTECTED]".to_string());
    } else if security::is_secret_ref_marker(&value) {
        value = Value::String("[SECRET_REF]".to_string());
    } else if let Err(e) = security::mask_all(&mut value) {
        set_last_error(e.to_string());
        return BCS_ERR_FORMAT;
    } else if let Err(e) = security::mask_secret_refs(&mut value) {
        set_last_error(e.to_string());
        return BCS_ERR_FORMAT;
    }

    let json_value = match value_to_json(&value) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e.to_string());
            return BCS_ERR_FORMAT;
        }
    };
    let json = match serde_json::to_string(&json_value) {
        Ok(s) => s,
        Err(e) => {
            set_last_error(e.to_string());
            return BCS_ERR_FORMAT;
        }
    };
    alloc_cstring(json, out_json)
}

/// Export agent-safe schema JSON from BCS bytes (no data-layer values).
///
/// # Safety
/// Same pointer rules as [`bcs_decode_to_json`].
#[no_mangle]
pub unsafe extern "C" fn bcs_schema_export_json(
    data_ptr: *const u8,
    data_len: usize,
    out_json: *mut *mut c_char,
) -> c_int {
    clear_last_error();
    if data_ptr.is_null() {
        set_last_error("null data pointer");
        return BCS_ERR_NULL;
    }

    let data = slice::from_raw_parts(data_ptr, data_len);
    let mut decoder = match Decoder::from_bytes(data) {
        Ok(d) => d,
        Err(e) => {
            set_last_error(e.to_string());
            return BCS_ERR_FORMAT;
        }
    };

    let schema = match decoder.schema() {
        Ok(s) => s,
        Err(e) => {
            set_last_error(e.to_string());
            return BCS_ERR_FORMAT;
        }
    };

    let json = match schema.to_agent_safe_json() {
        Ok(s) => s,
        Err(e) => {
            set_last_error(e.to_string());
            return BCS_ERR_FORMAT;
        }
    };
    alloc_cstring(json, out_json)
}

/// Validate a BCS buffer. Writes `1` to `*out_ok` when valid, else `0`.
///
/// # Safety
/// - `data_ptr` must point to `data_len` readable bytes.
/// - `out_ok` must be a valid writable pointer.
#[no_mangle]
pub unsafe extern "C" fn bcs_validate(
    data_ptr: *const u8,
    data_len: usize,
    out_ok: *mut c_int,
) -> c_int {
    clear_last_error();
    if data_ptr.is_null() || out_ok.is_null() {
        set_last_error("null pointer argument");
        return BCS_ERR_NULL;
    }

    let data = slice::from_raw_parts(data_ptr, data_len);
    match Decoder::from_bytes(data) {
        Ok(mut decoder) => match decoder.decode_to_value() {
            Ok(_) => {
                *out_ok = 1;
                BCS_OK
            }
            Err(e) => {
                set_last_error(e.to_string());
                *out_ok = 0;
                BCS_OK
            }
        },
        Err(e) => {
            set_last_error(e.to_string());
            *out_ok = 0;
            BCS_OK
        }
    }
}

/// Protect sensitive JSON paths with password (`pbkdf2`) and re-encode as BCS.
///
/// # Safety
/// All string pointers must be valid NUL-terminated UTF-8. Output pointers must be writable.
#[no_mangle]
pub unsafe extern "C" fn bcs_protect_json(
    json_ptr: *const c_char,
    paths_csv_ptr: *const c_char,
    password_ptr: *const c_char,
    compact: c_int,
    compress_data: c_int,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> c_int {
    bcs_protect_json_ex(
        json_ptr,
        paths_csv_ptr,
        password_ptr,
        ptr::null(),
        ptr::null(),
        None,
        ptr::null_mut(),
        compact,
        compress_data,
        out_ptr,
        out_len,
    )
}

/// Protect paths with password (`pbkdf2`) or KMS wrap (`kms`).
///
/// If `password_ptr` is non-null, uses `pbkdf2`. Otherwise requires
/// `kms_provider_ptr`, `kms_key_ptr`, and `wrap_fn`.
///
/// # Safety
/// All string pointers must be valid NUL-terminated UTF-8 when non-null.
#[no_mangle]
pub unsafe extern "C" fn bcs_protect_json_ex(
    json_ptr: *const c_char,
    paths_csv_ptr: *const c_char,
    password_ptr: *const c_char,
    kms_provider_ptr: *const c_char,
    kms_key_ptr: *const c_char,
    wrap_fn: BcsKeyWrapFn,
    userdata: *mut c_void,
    compact: c_int,
    compress_data: c_int,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> c_int {
    clear_last_error();
    let json = match cstr_to_str(json_ptr) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let paths_csv = match cstr_to_str(paths_csv_ptr) {
        Ok(s) => s,
        Err(code) => return code,
    };

    let paths: Vec<String> = paths_csv
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect();
    if paths.is_empty() {
        set_last_error("no sensitive paths provided");
        return BCS_ERR_INVALID_ARG;
    }

    let mut config = EncoderConfig::default();
    if compact != 0 {
        config.include_semantic_layer = false;
        config.include_index_table = false;
        config.compression = false;
    }
    config.data_compression = compress_data != 0;

    let mut encoder = Encoder::with_config(config);
    let encoded = match encoder.encode_from_json(json) {
        Ok(b) => b,
        Err(e) => {
            set_last_error(e.to_string());
            return BCS_ERR_FORMAT;
        }
    };

    let mut decoder = match Decoder::from_bytes(&encoded) {
        Ok(d) => d,
        Err(e) => {
            set_last_error(e.to_string());
            return BCS_ERR_FORMAT;
        }
    };
    let source_config = EncoderConfig::from_header(decoder.header());
    let mut value = match decoder.decode_to_value() {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e.to_string());
            return BCS_ERR_FORMAT;
        }
    };

    if !password_ptr.is_null() {
        let password = match cstr_to_str(password_ptr) {
            Ok(s) => s,
            Err(code) => return code,
        };
        if password.is_empty() {
            set_last_error("password cannot be empty");
            return BCS_ERR_INVALID_ARG;
        }
        if let Err(e) = security::protect_paths(&mut value, &paths, password) {
            set_last_error(e.to_string());
            return BCS_ERR_FORMAT;
        }
    } else {
        let provider = match cstr_to_str(kms_provider_ptr) {
            Ok(s) => s,
            Err(code) => return code,
        };
        let kek = match cstr_to_str(kms_key_ptr) {
            Ok(s) => s,
            Err(code) => return code,
        };
        let Some(wrap_fn) = wrap_fn else {
            set_last_error("KMS wrap callback is required when password is null");
            return BCS_ERR_INVALID_ARG;
        };
        let wrapper = FfiCallbackKeyWrapper {
            wrap_fn: Some(wrap_fn),
            unwrap_fn: None,
            userdata,
        };
        if let Err(e) = security::protect_paths_kms(&mut value, &paths, provider, kek, &wrapper) {
            set_last_error(e.to_string());
            return BCS_ERR_FORMAT;
        }
    }

    let json_value = match value_to_json(&value) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(e.to_string());
            return BCS_ERR_FORMAT;
        }
    };
    let protected_json = match serde_json::to_string(&json_value) {
        Ok(s) => s,
        Err(e) => {
            set_last_error(e.to_string());
            return BCS_ERR_FORMAT;
        }
    };

    let mut out_encoder = Encoder::with_config(source_config);
    match out_encoder.encode_from_json(&protected_json) {
        Ok(bytes) => alloc_bytes(bytes, out_ptr, out_len),
        Err(e) => {
            set_last_error(e.to_string());
            BCS_ERR_FORMAT
        }
    }
}

/// Free a buffer allocated by encode/protect functions.
///
/// # Safety
/// `ptr` must be null or previously returned with the same `len`.
#[no_mangle]
pub unsafe extern "C" fn bcs_free_buffer(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    let _ = Vec::from_raw_parts(ptr, len, len);
}

/// Free a C string allocated by decode/path functions.
///
/// # Safety
/// `ptr` must be null or previously returned by this library.
#[no_mangle]
pub unsafe extern "C" fn bcs_free_string(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    let _ = CString::from_raw(ptr);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn encode_decode_roundtrip() {
        let json = CString::new(r#"{"a":1,"b":"x"}"#).unwrap();
        let mut out_ptr: *mut u8 = ptr::null_mut();
        let mut out_len: usize = 0;
        let rc = unsafe { bcs_encode_json(json.as_ptr(), 0, 0, &mut out_ptr, &mut out_len) };
        assert_eq!(rc, BCS_OK);
        assert!(!out_ptr.is_null());
        assert!(out_len > 0);

        let mut out_json: *mut c_char = ptr::null_mut();
        let rc = unsafe { bcs_decode_to_json(out_ptr, out_len, ptr::null(), &mut out_json) };
        assert_eq!(rc, BCS_OK);
        let decoded = unsafe { CStr::from_ptr(out_json) }.to_str().unwrap();
        assert!(decoded.contains("\"a\""));

        unsafe {
            bcs_free_string(out_json);
            bcs_free_buffer(out_ptr, out_len);
        }
    }

    #[test]
    fn schema_export_is_agent_safe() {
        let json = CString::new(r#"{"database":{"password":"secret"},"host":"db"}"#).unwrap();
        let mut out_ptr: *mut u8 = ptr::null_mut();
        let mut out_len: usize = 0;
        assert_eq!(
            unsafe { bcs_encode_json(json.as_ptr(), 0, 0, &mut out_ptr, &mut out_len) },
            BCS_OK
        );

        let mut out_json: *mut c_char = ptr::null_mut();
        assert_eq!(
            unsafe { bcs_schema_export_json(out_ptr, out_len, &mut out_json) },
            BCS_OK
        );
        let exported = unsafe { CStr::from_ptr(out_json) }.to_str().unwrap();
        assert!(exported.contains("sensitive_paths") || exported.contains("paths"));
        assert!(!exported.contains("secret"));

        unsafe {
            bcs_free_string(out_json);
            bcs_free_buffer(out_ptr, out_len);
        }
    }

    #[test]
    fn path_query_works() {
        let json = CString::new(r#"{"server":{"host":"localhost"}}"#).unwrap();
        let mut out_ptr: *mut u8 = ptr::null_mut();
        let mut out_len: usize = 0;
        assert_eq!(
            unsafe { bcs_encode_json(json.as_ptr(), 0, 0, &mut out_ptr, &mut out_len) },
            BCS_OK
        );

        let path = CString::new("server.host").unwrap();
        let mut out_json: *mut c_char = ptr::null_mut();
        assert_eq!(
            unsafe { bcs_get_path_json(out_ptr, out_len, path.as_ptr(), &mut out_json) },
            BCS_OK
        );
        let value = unsafe { CStr::from_ptr(out_json) }.to_str().unwrap();
        assert_eq!(value, "\"localhost\"");

        unsafe {
            bcs_free_string(out_json);
            bcs_free_buffer(out_ptr, out_len);
        }
    }

    unsafe extern "C" fn env_resolve(
        scheme: *const c_char,
        locator: *const c_char,
        _userdata: *mut c_void,
    ) -> *mut c_char {
        let scheme = CStr::from_ptr(scheme).to_str().unwrap_or("");
        let locator = CStr::from_ptr(locator).to_str().unwrap_or("");
        if scheme != "env" {
            return ptr::null_mut();
        }
        match std::env::var(locator) {
            Ok(value) => {
                let c = CString::new(value).unwrap();
                // Use library allocator so bcs_free_string can reclaim it.
                bcs_strdup(c.as_ptr())
            }
            Err(_) => ptr::null_mut(),
        }
    }

    #[test]
    fn decode_resolves_secret_refs_via_callback() {
        let marker = bcs_core::security::format_secret_ref("env", "BCS_FFI_TEST_TOKEN").unwrap();
        let json = CString::new(format!(r#"{{"api":{{"token":"{}"}}}}"#, marker)).unwrap();
        let mut out_ptr: *mut u8 = ptr::null_mut();
        let mut out_len: usize = 0;
        assert_eq!(
            unsafe { bcs_encode_json(json.as_ptr(), 0, 0, &mut out_ptr, &mut out_len) },
            BCS_OK
        );

        std::env::set_var("BCS_FFI_TEST_TOKEN", "tok_from_callback");

        let mut masked: *mut c_char = ptr::null_mut();
        assert_eq!(
            unsafe { bcs_decode_to_json(out_ptr, out_len, ptr::null(), &mut masked) },
            BCS_OK
        );
        let masked_s = unsafe { CStr::from_ptr(masked) }.to_str().unwrap();
        assert!(masked_s.contains("[SECRET_REF]"));
        unsafe { bcs_free_string(masked) };

        let mut resolved: *mut c_char = ptr::null_mut();
        assert_eq!(
            unsafe {
                bcs_decode_to_json_ex(
                    out_ptr,
                    out_len,
                    ptr::null(),
                    Some(env_resolve),
                    ptr::null_mut(),
                    None,
                    ptr::null_mut(),
                    &mut resolved,
                )
            },
            BCS_OK
        );
        let resolved_s = unsafe { CStr::from_ptr(resolved) }.to_str().unwrap();
        assert!(resolved_s.contains("tok_from_callback"));
        assert!(!resolved_s.contains("[SECRET_REF]"));

        unsafe {
            bcs_free_string(resolved);
            bcs_free_buffer(out_ptr, out_len);
        }
        std::env::remove_var("BCS_FFI_TEST_TOKEN");
    }

    unsafe extern "C" fn xor_wrap(
        _provider: *const c_char,
        _kek: *const c_char,
        dek: *const u8,
        dek_len: usize,
        out_wrapped: *mut *mut u8,
        out_wrapped_len: *mut usize,
        _userdata: *mut c_void,
    ) -> c_int {
        if dek.is_null() || dek_len == 0 || out_wrapped.is_null() || out_wrapped_len.is_null() {
            return BCS_ERR_NULL;
        }
        let src = slice::from_raw_parts(dek, dek_len);
        let buf = bcs_alloc(dek_len);
        if buf.is_null() {
            return BCS_ERR_INTERNAL;
        }
        for (i, b) in src.iter().enumerate() {
            *buf.add(i) = b ^ 0xA5;
        }
        *out_wrapped = buf;
        *out_wrapped_len = dek_len;
        BCS_OK
    }

    unsafe extern "C" fn xor_unwrap(
        _provider: *const c_char,
        _kek: *const c_char,
        wrapped: *const u8,
        wrapped_len: usize,
        out_dek: *mut u8,
        _userdata: *mut c_void,
    ) -> c_int {
        if wrapped.is_null() || out_dek.is_null() || wrapped_len != 32 {
            return BCS_ERR_INVALID_ARG;
        }
        let src = slice::from_raw_parts(wrapped, wrapped_len);
        for (i, b) in src.iter().enumerate() {
            *out_dek.add(i) = b ^ 0xA5;
        }
        BCS_OK
    }

    #[test]
    fn protect_and_decode_kms_via_callbacks() {
        let json = CString::new(r#"{"database":{"password":"s3cret"}}"#).unwrap();
        let paths = CString::new("database.password").unwrap();
        let provider = CString::new("cmd").unwrap();
        let key = CString::new("alias/test").unwrap();
        let mut out_ptr: *mut u8 = ptr::null_mut();
        let mut out_len: usize = 0;
        assert_eq!(
            unsafe {
                bcs_protect_json_ex(
                    json.as_ptr(),
                    paths.as_ptr(),
                    ptr::null(),
                    provider.as_ptr(),
                    key.as_ptr(),
                    Some(xor_wrap),
                    ptr::null_mut(),
                    0,
                    0,
                    &mut out_ptr,
                    &mut out_len,
                )
            },
            BCS_OK
        );

        let mut masked: *mut c_char = ptr::null_mut();
        assert_eq!(
            unsafe { bcs_decode_to_json(out_ptr, out_len, ptr::null(), &mut masked) },
            BCS_OK
        );
        let masked_s = unsafe { CStr::from_ptr(masked) }.to_str().unwrap();
        assert!(masked_s.contains("[PROTECTED]"));
        unsafe { bcs_free_string(masked) };

        let mut revealed: *mut c_char = ptr::null_mut();
        assert_eq!(
            unsafe {
                bcs_decode_to_json_ex(
                    out_ptr,
                    out_len,
                    ptr::null(),
                    None,
                    ptr::null_mut(),
                    Some(xor_unwrap),
                    ptr::null_mut(),
                    &mut revealed,
                )
            },
            BCS_OK
        );
        let revealed_s = unsafe { CStr::from_ptr(revealed) }.to_str().unwrap();
        assert!(revealed_s.contains("s3cret"));
        assert!(!revealed_s.contains("[PROTECTED]"));

        unsafe {
            bcs_free_string(revealed);
            bcs_free_buffer(out_ptr, out_len);
        }
    }
}

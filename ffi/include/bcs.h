#ifndef BCS_H
#define BCS_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Status codes */
#define BCS_OK              0
#define BCS_ERR_NULL        1
#define BCS_ERR_UTF8        2
#define BCS_ERR_FORMAT      3
#define BCS_ERR_INVALID_ARG 4
#define BCS_ERR_INTERNAL    5

/**
 * Last error message for the calling thread.
 * Valid until the next failing BCS call on this thread.
 */
const char *bcs_last_error(void);

/** Library version string, e.g. "0.1.0". */
const char *bcs_version(void);

/**
 * Encode JSON text to BCS bytes.
 * On success, *out_ptr / *out_len are set; free with bcs_free_buffer.
 */
int bcs_encode_json(
    const char *json_ptr,
    int compact,
    int compress_data,
    uint8_t **out_ptr,
    size_t *out_len
);

/**
 * Decode BCS bytes to a newly allocated JSON string.
 * password_ptr may be NULL (protected fields stay masked).
 * Secret refs are masked as "[SECRET_REF]" (use bcs_decode_to_json_ex to resolve).
 * Free result with bcs_free_string.
 */
int bcs_decode_to_json(
    const uint8_t *data_ptr,
    size_t data_len,
    const char *password_ptr,
    char **out_json
);

/**
 * Host callback for resolving `__bcs_secret_ref__:` markers.
 *
 * Must return a string allocated with bcs_strdup (or compatible with
 * bcs_free_string), or NULL on failure. BCS frees the returned pointer.
 */
typedef char *(*bcs_secret_resolve_fn)(
    const char *scheme,
    const char *locator,
    void *userdata
);

/**
 * Host callback to wrap a DEK for the `kms` protect scheme.
 * On success return 0 and set *out_wrapped / *out_wrapped_len via bcs_alloc.
 * On failure return non-zero.
 */
typedef int (*bcs_key_wrap_fn)(
    const char *provider,
    const char *kek_locator,
    const uint8_t *dek,
    size_t dek_len,
    uint8_t **out_wrapped,
    size_t *out_wrapped_len,
    void *userdata
);

/**
 * Host callback to unwrap a DEK for the `kms` protect scheme.
 * Write exactly 32 bytes to out_dek on success and return 0.
 */
typedef int (*bcs_key_unwrap_fn)(
    const char *provider,
    const char *kek_locator,
    const uint8_t *wrapped,
    size_t wrapped_len,
    uint8_t *out_dek,
    void *userdata
);

/**
 * Decode with optional password reveal, secret-ref resolve, and KMS unwrap.
 * resolve_fn / unwrap_fn may be NULL.
 * Free result with bcs_free_string.
 */
int bcs_decode_to_json_ex(
    const uint8_t *data_ptr,
    size_t data_len,
    const char *password_ptr,
    bcs_secret_resolve_fn resolve_fn,
    void *resolve_userdata,
    bcs_key_unwrap_fn unwrap_fn,
    void *unwrap_userdata,
    char **out_json
);

/**
 * Allocate a copy of `s` for FFI callback return values.
 * Free with bcs_free_string. Returns NULL on allocation failure.
 */
char *bcs_strdup(const char *s);

/** Allocate `len` bytes for FFI callback outputs. Free with bcs_free_buffer. */
uint8_t *bcs_alloc(size_t len);

/**
 * Resolve a path query and return JSON for the matched value.
 * Free result with bcs_free_string.
 */
int bcs_get_path_json(
    const uint8_t *data_ptr,
    size_t data_len,
    const char *path_ptr,
    char **out_json
);

/**
 * Export agent-safe schema JSON from BCS bytes (paths, types, sensitive; never values).
 * Free result with bcs_free_string.
 */
int bcs_schema_export_json(
    const uint8_t *data_ptr,
    size_t data_len,
    char **out_json
);

/**
 * Validate BCS bytes. Writes 1 to *out_ok when decodable, else 0.
 * Always returns BCS_OK unless arguments are invalid.
 */
int bcs_validate(
    const uint8_t *data_ptr,
    size_t data_len,
    int *out_ok
);

/**
 * Protect comma-separated JSON paths with password (`pbkdf2`) and encode to BCS.
 * Free result with bcs_free_buffer.
 */
int bcs_protect_json(
    const char *json_ptr,
    const char *paths_csv_ptr,
    const char *password_ptr,
    int compact,
    int compress_data,
    uint8_t **out_ptr,
    size_t *out_len
);

/**
 * Protect paths with either password (`pbkdf2`) or KMS wrap (`kms`).
 * If password_ptr is non-NULL, uses pbkdf2 (wrap_fn ignored).
 * Else requires kms_provider_ptr, kms_key_ptr, and wrap_fn.
 */
int bcs_protect_json_ex(
    const char *json_ptr,
    const char *paths_csv_ptr,
    const char *password_ptr,
    const char *kms_provider_ptr,
    const char *kms_key_ptr,
    bcs_key_wrap_fn wrap_fn,
    void *userdata,
    int compact,
    int compress_data,
    uint8_t **out_ptr,
    size_t *out_len
);

void bcs_free_buffer(uint8_t *ptr, size_t len);
void bcs_free_string(char *ptr);

#ifdef __cplusplus
}
#endif

#endif /* BCS_H */

# BCS Python bindings

Thin `ctypes` wrapper around `libbcs_ffi`.

> Full guide (generate natives + all languages): **[docs/bindings.md](../../docs/bindings.md)**

## Prerequisites

```bash
# From repo root
cargo build -p bcs-ffi --release
# optional packaged layout
./scripts/package-ffi.sh
```

Or point to the shared library:

```bash
export BCS_FFI_LIB=/absolute/path/to/libbcs_ffi.dylib
```

## Usage

```python
import os
from bcs import encode_json, decode_to_json, get_path_json, validate, protect_json

data = encode_json('{"server":{"host":"localhost"}}')
assert validate(data)
print(get_path_json(data, "server.host"))  # "localhost"

protected = protect_json(
    '{"database":{"password":"secret"}}',
    ["database.password"],
    password="master",
)
print(decode_to_json(protected))                 # masked
print(decode_to_json(protected, password="master"))  # revealed

# Secret refs: masked by default; pass resolve_secrets=callable to resolve
ref = encode_json('{"t":"__bcs_secret_ref__:env:API_TOKEN"}')
print(decode_to_json(ref))  # [SECRET_REF]
print(decode_to_json(ref, resolve_secrets=lambda s, n: os.environ.get(n) if s == "env" else None))

# KMS scheme via wrap/unwrap callables
def xor_wrap(_p, _k, dek): return bytes(b ^ 0xA5 for b in dek)
def xor_unwrap(_p, _k, wrapped): return bytes(b ^ 0xA5 for b in wrapped)
kms = protect_json('{"t":"x"}', ["t"], kms_provider="cmd", kms_key="k1", wrap_key=xor_wrap)
print(decode_to_json(kms, unwrap_key=xor_unwrap))
```
## Install (editable, local)

```bash
cd bindings/python
pip install -e .
python -c "import bcs; print(bcs.version())"
```

## Self-test

```bash
PYTHONPATH=bindings/python python -m bcs
# or
python bindings/python/bcs/__init__.py
```

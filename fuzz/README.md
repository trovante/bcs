# BCS Fuzzing

Fuzz targets live in this crate (excluded from the main workspace).

## Prerequisites

```bash
rustup install nightly
cargo install cargo-fuzz
```

## Targets

| Target | What it exercises |
|---|---|
| `decode_bytes` | Header parse, full decode, schema/index load, JSON export |
| `get_path` | Indexed/nested path lookup on arbitrary bytes |

## Run locally

```bash
# From repository root
cargo +nightly fuzz run decode_bytes
cargo +nightly fuzz run get_path

# Optional: time-bounded CI-style smoke
cargo +nightly fuzz run decode_bytes -- -max_total_time=60
```

## Notes

- Targets must not panic on malformed input; depth/length limits in `bcs-core`
  are part of what fuzzing validates.
- Deterministic adversarial coverage (without libFuzzer) remains in
  `core/tests/adversarial_decode_test.rs`.
- Weekly CI runs a short `decode_bytes` smoke under `.github/workflows/security.yml`.

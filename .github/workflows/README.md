# GitHub Actions Workflows

This directory contains Rust-focused CI/CD workflows for BCS.

## CI (`ci.yml`)

Trigger: push to `main`/`develop` and pull requests.

Runs:

- Core tests (`bcs-core`) on Linux/macOS/Windows (stable + beta)
- CLI tests (`bcs-cli`) on Linux/macOS/Windows
- Integration tests
- Rust lint/format checks

## Release (`release.yml`)

Trigger: tags `v*.*.*`.

Runs:

- Create GitHub release
- Build CLI binaries for Linux/macOS/Windows
- Publish Rust crates to crates.io

## Documentation (`docs.yml`)

Trigger: docs-related changes on `main` and manual dispatch.

Runs:

- Build Rust API docs
- Build docs site
- Deploy to GitHub Pages

## Benchmark (`benchmark.yml`)

Runs benchmark jobs and reports regressions.

## Security (`security.yml`)

Runs:

- `cargo audit`
- dependency review on PRs

## Required Secrets

- `CARGO_REGISTRY_TOKEN`

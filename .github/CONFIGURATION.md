# GitHub Configuration

This directory contains GitHub configuration for the Rust-only BCS project.

## Included Workflows

- `workflows/ci.yml`: CI for Rust core and CLI.
- `workflows/release.yml`: Builds CLI binaries and publishes Rust crates.
- `workflows/docs.yml`: Builds and deploys documentation.
- `workflows/benchmark.yml`: Runs benchmark jobs.
- `workflows/security.yml`: Runs security checks for Rust dependencies.

## Required Secrets

- `CARGO_REGISTRY_TOKEN`: Token used to publish Rust crates.

`GITHUB_TOKEN` is provided automatically by GitHub Actions.

## Useful Links

- `release-checklist.md`
- `workflows/README.md`
- `CI-CD-SETUP.md`
- `CI-CD-SUMMARY.md`

# CI/CD Setup Guide

This guide explains how to configure CI/CD for the Rust-only BCS project.

## Overview

The repository uses GitHub Actions for:

- CI on push/PR (core + CLI + integration + lint)
- Release automation for Rust crates and CLI binaries
- Documentation deployment
- Benchmark execution
- Security scanning

## Prerequisites

- GitHub repository with admin access
- crates.io account

## 1) Enable GitHub Actions

In repository settings, ensure Actions are enabled.

## 2) Configure GitHub Pages

For docs deployment:

1. Go to Settings -> Pages
2. Set source to GitHub Actions

## 3) Configure crates.io publishing token

1. Create token at `https://crates.io/settings/tokens`
2. Add repository secret:
   - Name: `CARGO_REGISTRY_TOKEN`
   - Value: token value

## 4) Verify CI

Open a PR and confirm `CI` workflow passes:

- `test-core`
- `test-cli`
- `integration-tests`
- `lint`

## 5) Create a release

1. Bump versions:

```bash
./scripts/bump-version.sh 0.1.0
```

2. Commit changes and push.
3. Tag release:

```bash
git tag -a v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0
```

4. Monitor `release.yml` in GitHub Actions.

## 6) Branch protection

Protect `main` and require at least:

- `test-core`
- `test-cli`
- `integration-tests`
- `lint`

## Troubleshooting

- If CI fails, inspect workflow logs and reproduce locally with `cargo test --all` and `cargo clippy --all-targets --all-features -- -D warnings`.
- If release fails, verify `CARGO_REGISTRY_TOKEN` and crate version uniqueness.

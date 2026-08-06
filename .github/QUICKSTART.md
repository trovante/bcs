# CI/CD Quick Start Guide

Get the Rust-only CI/CD pipeline running quickly.

## Prerequisites

- GitHub repository with admin access
- crates.io account

## Basic CI (5 minutes)

1. Push a branch and open a PR.
2. Confirm CI runs:
   - `test-core`
   - `test-cli`
   - `integration-tests`
   - `lint`
3. Enable branch protection requiring these checks.

## Full Setup (15 minutes)

1. Enable GitHub Pages (source: GitHub Actions).
2. Add `CARGO_REGISTRY_TOKEN` to repository secrets.
3. Create first release tag:

```bash
./scripts/bump-version.sh 0.1.0
git add .
git commit -m "chore: prepare release v0.1.0"
git push origin main
git tag -a v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0
```

4. Monitor `release.yml` in Actions.

## Useful Commands

```bash
./scripts/validate-workflows.sh
cargo test --all
cargo clippy --all-targets --all-features -- -D warnings
```

## Next Reads

- `.github/CI-CD-SETUP.md`
- `.github/release-checklist.md`
- `.github/workflows/README.md`

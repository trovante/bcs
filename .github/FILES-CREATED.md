# CI/CD Implementation - Files Created

This file tracks CI/CD files for the Rust-only BCS setup.

## Workflows

- `.github/workflows/ci.yml`
- `.github/workflows/release.yml`
- `.github/workflows/docs.yml`
- `.github/workflows/benchmark.yml`
- `.github/workflows/security.yml`

## GitHub Config and Docs

- `.github/README.md`
- `.github/CI-CD-SETUP.md`
- `.github/CI-CD-SUMMARY.md`
- `.github/QUICKSTART.md`
- `.github/release-checklist.md`
- `.github/dependabot.yml`
- `.github/workflows/README.md`

## Templates

- `.github/pull_request_template.md`
- `.github/ISSUE_TEMPLATE/bug_report.md`
- `.github/ISSUE_TEMPLATE/feature_request.md`
- `.github/ISSUE_TEMPLATE/config.yml`

## Scripts

- `scripts/bump-version.sh`
- `scripts/validate-workflows.sh`

## Notes

- CI/CD covers Rust workspace packages plus Linux binding self-tests (Python + TypeScript via FFI).
- Additional language wrappers (Swift/C#/Java) remain available locally via `scripts/run-binding-selftests.sh`.

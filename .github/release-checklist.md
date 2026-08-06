# Release Checklist

Use this checklist when preparing a BCS release (`0.1.x` alpha).

## Pre-release

- [ ] All CI jobs pass on `main`
- [ ] `CHANGELOG.md` cut for the version (move `[Unreleased]` items)
- [ ] Version bumped with `./scripts/bump-version.sh X.Y.Z` (workspace + binding metadata)
- [ ] Path deps still carry matching `version = "X.Y.Z"` for crates.io
- [ ] `./scripts/pre-release-check.sh` passes locally
- [ ] Security review for scan/run/MCP/agent-safe still current
      ([docs/adr/security-review-followups.md](../docs/adr/security-review-followups.md))
- [ ] Repo secret `CARGO_REGISTRY_TOKEN` is set

## Release process

1. Commit release prep on `main` (or merge release PR).
2. Create and push tag:

```bash
git tag -a vX.Y.Z -m "Release vX.Y.Z"
git push origin vX.Y.Z
```

3. Monitor `release.yml` for:
   - [ ] GitHub Release with CLI binaries (linux/mac/windows)
   - [ ] crates.io: `bcs-core` → `bcs-secrets` → `bcs-cli` → `bcs-ffi` → `bcs-mcp`

## Post-release

- [ ] `cargo install bcs-cli` and `cargo install bcs-mcp` work
- [ ] Release assets download correctly
- [ ] Announce as **alpha** (format/API may change in `0.1.x`)

#!/bin/bash
set -euo pipefail

# Bump version across Rust workspace crates (and TS/Python binding metadata).

if [ $# -ne 1 ]; then
    echo "Usage: $0 <new-version>"
    echo "Example: $0 0.1.1"
    exit 1
fi

NEW_VERSION=$1

if ! [[ $NEW_VERSION =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "Error: Version must be in format X.Y.Z (e.g. 0.1.1)"
    exit 1
fi

echo "Bumping version to $NEW_VERSION..."

update_crate_version() {
    local file=$1
    sed -i.bak "s/^version = \".*\"/version = \"$NEW_VERSION\"/" "$file"
    # Keep path deps in lockstep for crates.io publish
    sed -i.bak -E "s/(bcs-(core|secrets) = \{ path = \"[^\"]+\", version = \")[^\"]+(\")/\1$NEW_VERSION\3/" "$file"
}

update_crate_version Cargo.toml
update_crate_version core/Cargo.toml
update_crate_version cli/Cargo.toml
update_crate_version secrets/Cargo.toml
update_crate_version ffi/Cargo.toml
update_crate_version mcp/Cargo.toml

if [ -f bindings/typescript/package.json ]; then
    sed -i.bak "s/\"version\": \"[^\"]*\"/\"version\": \"$NEW_VERSION\"/" bindings/typescript/package.json
fi

if [ -f bindings/python/pyproject.toml ]; then
    sed -i.bak "s/^version = \".*\"/version = \"$NEW_VERSION\"/" bindings/python/pyproject.toml
fi

echo "Refreshing Cargo.lock..."
cargo update --workspace

find . -name "*.bak" -type f -delete

echo "✓ Version bumped to $NEW_VERSION"
echo ""
echo "Next steps:"
echo "1. Review changes: git diff"
echo "2. Update CHANGELOG.md ([Unreleased] → new section)"
echo "3. Commit: git commit -am \"chore: bump version to $NEW_VERSION\""
echo "4. Tag: git tag -a v$NEW_VERSION -m \"Release v$NEW_VERSION\""
echo "5. Push: git push origin main --tags"

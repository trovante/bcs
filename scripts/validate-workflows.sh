#!/bin/bash
set -e

echo "Validating GitHub Actions workflows..."

# Check if workflows directory exists
if [ ! -d ".github/workflows" ]; then
    echo "Error: .github/workflows directory not found"
    exit 1
fi

# Count workflow files
WORKFLOW_COUNT=$(find .github/workflows -name "*.yml" -type f | wc -l)
echo "Found $WORKFLOW_COUNT workflow files"

# List all workflows
echo ""
echo "Workflows:"
find .github/workflows -name "*.yml" -type f | while read -r file; do
    echo "  - $(basename "$file")"
done

# Check for required workflows
echo ""
echo "Checking required workflows..."
REQUIRED_WORKFLOWS=("ci.yml" "release.yml" "docs.yml")
for workflow in "${REQUIRED_WORKFLOWS[@]}"; do
    if [ -f ".github/workflows/$workflow" ]; then
        echo "  ✓ $workflow"
    else
        echo "  ✗ $workflow (missing)"
        exit 1
    fi
done

# Check for required secrets documentation
echo ""
echo "Checking documentation..."
if [ -f ".github/workflows/README.md" ]; then
    echo "  ✓ Workflows README"
else
    echo "  ✗ Workflows README (missing)"
fi

if [ -f ".github/CI-CD-SETUP.md" ]; then
    echo "  ✓ CI/CD Setup Guide"
else
    echo "  ✗ CI/CD Setup Guide (missing)"
fi

if [ -f ".github/release-checklist.md" ]; then
    echo "  ✓ Release Checklist"
else
    echo "  ✗ Release Checklist (missing)"
fi

# Check for dependabot config
echo ""
echo "Checking Dependabot configuration..."
if [ -f ".github/dependabot.yml" ]; then
    echo "  ✓ Dependabot configured"
else
    echo "  ✗ Dependabot not configured"
fi

# Check for issue templates
echo ""
echo "Checking issue templates..."
if [ -d ".github/ISSUE_TEMPLATE" ]; then
    TEMPLATE_COUNT=$(find .github/ISSUE_TEMPLATE -name "*.md" -type f | wc -l)
    echo "  ✓ Found $TEMPLATE_COUNT issue templates"
else
    echo "  ✗ No issue templates found"
fi

# Check for PR template
if [ -f ".github/pull_request_template.md" ]; then
    echo "  ✓ Pull request template"
else
    echo "  ✗ Pull request template (missing)"
fi

echo ""
echo "✓ Validation complete!"
echo ""
echo "Next steps:"
echo "1. Review workflows in .github/workflows/"
echo "2. Set up secrets following .github/CI-CD-SETUP.md"
echo "3. Enable GitHub Pages in repository settings"
echo "4. Test CI by creating a pull request"
echo "5. Create first release with: ./scripts/bump-version.sh 0.1.0"

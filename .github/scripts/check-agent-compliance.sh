#!/usr/bin/env bash
# Check agent compliance block in PR description
# Usage: PR_BODY="..." BASE_SHA="..." HEAD_SHA="..." ./check-agent-compliance.sh

set -euo pipefail

: "${PR_BODY:?PR_BODY environment variable required}"
: "${BASE_SHA:?BASE_SHA environment variable required}"
: "${HEAD_SHA:?HEAD_SHA environment variable required}"

# Skip check if PR is not from an agent (no compliance block expected)
if ! echo "$PR_BODY" | grep -q "agent_compliance:"; then
    echo "No agent_compliance block found - assuming human PR, skipping check"
    exit 0
fi

echo "Agent compliance block detected, validating..."

# Extract YAML block between markers
compliance_yaml=$(echo "$PR_BODY" | sed -n '/^# --- Agent Compliance ---$/,/^```$/p' | tail -n +2 | head -n -1)

if [ -z "$compliance_yaml" ]; then
    echo "::error::Agent compliance block found but malformed. Expected format:"
    cat <<'EOF'
# --- Agent Compliance ---
agent_compliance:
  docs_read: [AGENTS.md]
  constraints_followed: true
  files_modified: [list of files]
  deviations: [none or explanations]
EOF
    exit 1
fi

# Validate required fields exist
missing_fields=()
echo "$compliance_yaml" | grep -q "docs_read:" || missing_fields+=("docs_read")
echo "$compliance_yaml" | grep -q "constraints_followed:" || missing_fields+=("constraints_followed")
echo "$compliance_yaml" | grep -q "files_modified:" || missing_fields+=("files_modified")

if [ ${#missing_fields[@]} -gt 0 ]; then
    echo "::error::Missing required fields in agent compliance block: ${missing_fields[*]}"
    exit 1
fi

# Check AGENTS.md is in docs_read
if ! echo "$compliance_yaml" | grep -A5 "docs_read:" | grep -q "AGENTS.md"; then
    echo "::error::AGENTS.md must be listed in docs_read"
    exit 1
fi

# Get actual changed files
actual_files=$(git diff --name-only "$BASE_SHA".."$HEAD_SHA" | sort)
echo "Files actually modified:"
echo "$actual_files"

echo "Agent compliance block validated successfully"

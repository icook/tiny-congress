#!/usr/bin/env bash
set -euo pipefail

# One-time script to seed refine-ledger.toml from existing merged refinement PRs.
# Usage: ./scripts/seed-ledger.sh

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LEDGER="$REPO_ROOT/refine-ledger.toml"
FOCUS_PATH="service/src/identity/"

echo "Seeding ledger from merged refinement PRs..."

# Initialize ledger
cat > "$LEDGER" << 'INIT'
# Auto-maintained by refine.sh — manual edits are fine for overrides
INIT

area_key="$FOCUS_PATH"
now="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

yq -i -p toml -o toml ".areas.\"$area_key\".status = \"active\"" "$LEDGER"
yq -i -p toml -o toml ".areas.\"$area_key\".total_prs = 0" "$LEDGER"
yq -i -p toml -o toml ".areas.\"$area_key\".total_skips = 0" "$LEDGER"
yq -i -p toml -o toml ".areas.\"$area_key\".consecutive_idle = 0" "$LEDGER"
yq -i -p toml -o toml ".areas.\"$area_key\".last_run = \"$now\"" "$LEDGER"
yq -i -p toml -o toml ".areas.\"$area_key\".graduated_at = \"\"" "$LEDGER"

# Fetch merged refinement PRs
pr_count=0
while IFS=$'\t' read -r number title; do
    # Infer type from title
    type="unknown"
    case "$title" in
        *test*|*Test*) type="test_coverage" ;;
        *newtype*|*Newtype*|*consolidate*|*thiserror*|*pattern*) type="pattern_enforcement" ;;
        *security*|*enforce*|*max-length*|*anti-enumeration*) type="security_hardening" ;;
        *cleanup*|*dead*|*unused*|*simplif*) type="code_cleanup" ;;
    esac

    # Sanitize title for TOML (escape quotes)
    safe_title="$(echo "$title" | sed 's/"/\\"/g' | cut -c1-120)"

    yq -i -p toml -o toml \
        ".areas.\"$area_key\".history += [{\"pr\": $number, \"type\": \"$type\", \"impact\": \"medium\", \"summary\": \"$safe_title\"}]" \
        "$LEDGER"
    pr_count=$((pr_count + 1))
done < <(gh pr list --label "refinement" --state merged \
    --json number,title \
    --jq '.[] | [.number, .title] | @tsv')

yq -i -p toml -o toml ".areas.\"$area_key\".total_prs = $pr_count" "$LEDGER"

echo "Seeded $pr_count PRs into $LEDGER"
echo "Review the file and commit when satisfied."

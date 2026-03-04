#!/usr/bin/env bash
set -euo pipefail

# One-time script to seed refine-ledger.json from existing merged refinement PRs.
# Usage: ./scripts/seed-ledger.sh

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LEDGER="$REPO_ROOT/refine-ledger.json"
FOCUS_PATH="service/src/identity/"

echo "Seeding ledger from merged refinement PRs..."

area_key="$FOCUS_PATH"
now="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

# Initialize ledger with empty area
local_tmp="$(jq -n --arg key "$area_key" --arg now "$now" \
    '{areas: {($key): {status: "active", total_prs: 0, total_skips: 0, consecutive_idle: 0, last_run: $now, graduated_at: "", history: [], skipped: []}}}')"
echo "$local_tmp" > "$LEDGER"

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

    local_tmp="$(jq --arg key "$area_key" \
        --argjson pr "$number" \
        --arg type "$type" \
        --arg summary "$title" \
        '.areas[$key].history += [{pr: $pr, type: $type, impact: "medium", summary: $summary}]' \
        "$LEDGER")"
    echo "$local_tmp" > "$LEDGER"
    pr_count=$((pr_count + 1))
done < <(gh pr list --label "refinement" --state merged \
    --json number,title \
    --jq '.[] | [.number, .title] | @tsv')

local_tmp="$(jq --arg key "$area_key" --argjson count "$pr_count" '.areas[$key].total_prs = $count' "$LEDGER")"
echo "$local_tmp" > "$LEDGER"

echo "Seeded $pr_count PRs into $LEDGER"
echo "Review the file and commit when satisfied."

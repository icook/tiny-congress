# Refine Loop Improvements Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add impact scoring, a refinement ledger, and improved stopping logic to the automated refinement loop.

**Architecture:** Extend the existing `refine.sh` loop with a `skip` action, self-assessed impact scores, and a persistent `refine-ledger.toml` that accumulates knowledge across runs. The ledger is read at iteration start and written after each iteration.

**Tech Stack:** Bash, yq (TOML), jq (JSON), existing `refine.sh` infrastructure.

---

### Task 1: Add `min_impact` to `refine.toml`

**Files:**
- Modify: `refine.toml:14-20`

**Step 1: Add the config key**

Add `min_impact` to the `[behavior]` section:

```toml
[behavior]
cooldown = 0
max_prs = 0
idle_limit = 3
# Minimum impact to create a PR: "low" = everything, "medium" = skip cleanup, "high" = only significant
min_impact = "medium"
```

**Step 2: Commit**

```bash
git add refine.toml
git commit -m "refine: add min_impact config for impact scoring"
```

---

### Task 2: Update prompt with `skip` action, `impact` field, and ledger context

**Files:**
- Modify: `scripts/refine-prompt.md`

**Step 1: Add impact assessment rule**

Insert after rule 1 ("Find the SINGLE highest-value improvement"):

```markdown
2. Before making any change, assess its impact:
   - **high**: Fixes a real bug, security gap, or missing test for a failure mode.
   - **medium**: Enforces a project pattern or adds meaningful test coverage.
   - **low**: Cleanup, style, or marginal test improvement.
   If the improvement is below the configured threshold, output `"skip"` instead of making changes.
```

Renumber subsequent rules (old 2→3, 3→4, etc.).

**Step 2: Add ledger context section**

Insert before the "## Already in progress" section:

```markdown
## History for this focus area

{{LEDGER_CONTEXT}}

Do NOT re-implement anything in the history list.
Do NOT re-discover anything in the skipped list — these were evaluated and deemed below threshold.
```

**Step 3: Update output schema**

Replace the entire `## Output` section with:

```markdown
## Output

After you are done, your final text output MUST be valid JSON matching this schema:

\`\`\`json
{
  "action": "change" | "ticket" | "clean" | "skip",
  "impact": "high" | "medium" | "low",
  "summary": "one-sentence description of what you did or found",
  // Only if action is "skip":
  // "skip_reason": "why this wasn't worth a PR"
  // Only if action is "ticket":
  // "ticket_title": "short title for the GitHub issue",
  // "ticket_body": "markdown body explaining the design decision needed"
}
\`\`\`

- `"change"`: You made a change and committed it. Commit message MUST start with `refine:`.
- `"ticket"`: You found something that requires a design decision. Do NOT make changes.
- `"skip"`: You found an improvement but it's below the impact threshold. Do NOT make changes. Describe what you found.
- `"clean"`: Nothing worth improving in the focus area. No changes made.

`impact` is REQUIRED on all actions (including `clean` — rate what was found, even if nothing was worth doing).
```

**Step 4: Verify the prompt renders correctly**

```bash
cd scripts && ./refine.sh --dry-run 2>&1 | head -80
```

Expected: prompt renders with new sections, `{{LEDGER_CONTEXT}}` placeholder visible (empty ledger for now).

**Step 5: Commit**

```bash
git add scripts/refine-prompt.md
git commit -m "refine: add skip action, impact field, and ledger context to prompt"
```

---

### Task 3: Create ledger read/write functions in `refine.sh`

**Files:**
- Modify: `scripts/refine.sh`

**Step 1: Add ledger path and `min_impact` config parsing**

After line 35 (`IDLE_LIMIT=...`), add:

```bash
MIN_IMPACT="$(read_config '.behavior.min_impact')"
[[ -z "$MIN_IMPACT" || "$MIN_IMPACT" == "null" ]] && MIN_IMPACT="medium"

LEDGER="$REPO_ROOT/refine-ledger.toml"
```

**Step 2: Add impact threshold helper**

After the `log` function, add:

```bash
# Map impact levels to numeric values for comparison
impact_to_num() {
    case "$1" in
        high) echo 3 ;;
        medium) echo 2 ;;
        low) echo 1 ;;
        *) echo 0 ;;
    esac
}

IMPACT_THRESHOLD="$(impact_to_num "$MIN_IMPACT")"
```

**Step 3: Add `read_ledger_context` function**

After the `build_prompt` function (~line 155), add:

```bash
# ── Ledger management ──────────────────────────────────────────────

# Read ledger and build context string for the prompt.
# Returns markdown summary of history and skipped items for the focus area.
read_ledger_context() {
    if [[ ! -f "$LEDGER" ]]; then
        echo "No prior refinement history for this focus area."
        return 0
    fi

    local area_key
    area_key="$(echo "$FOCUS_PATH" | sed 's/\./\\./g')"

    local history=""
    local skipped=""

    # Read history entries
    local history_count
    history_count="$(yq -p toml -oy ".areas.\"$area_key\".history | length" "$LEDGER" 2>/dev/null || echo "0")"

    if [[ "$history_count" -gt 0 ]]; then
        history="### Completed improvements ($history_count total)"$'\n'
        # Show last 20 to keep prompt size reasonable
        local start=0
        [[ "$history_count" -gt 20 ]] && start=$((history_count - 20))
        for i in $(seq "$start" $((history_count - 1))); do
            local h_summary h_type h_impact h_pr
            h_summary="$(yq -p toml -oy ".areas.\"$area_key\".history[$i].summary" "$LEDGER" 2>/dev/null)"
            h_type="$(yq -p toml -oy ".areas.\"$area_key\".history[$i].type" "$LEDGER" 2>/dev/null)"
            h_impact="$(yq -p toml -oy ".areas.\"$area_key\".history[$i].impact" "$LEDGER" 2>/dev/null)"
            h_pr="$(yq -p toml -oy ".areas.\"$area_key\".history[$i].pr" "$LEDGER" 2>/dev/null)"
            history+="- [#${h_pr}] (${h_type}, ${h_impact}) ${h_summary}"$'\n'
        done
    fi

    # Read skipped entries
    local skipped_count
    skipped_count="$(yq -p toml -oy ".areas.\"$area_key\".skipped | length" "$LEDGER" 2>/dev/null || echo "0")"

    if [[ "$skipped_count" -gt 0 ]]; then
        skipped="### Skipped (below threshold)"$'\n'
        for i in $(seq 0 $((skipped_count - 1))); do
            local s_summary s_reason
            s_summary="$(yq -p toml -oy ".areas.\"$area_key\".skipped[$i].summary" "$LEDGER" 2>/dev/null)"
            s_reason="$(yq -p toml -oy ".areas.\"$area_key\".skipped[$i].reason" "$LEDGER" 2>/dev/null)"
            skipped+="- ${s_summary} (${s_reason})"$'\n'
        done
    fi

    if [[ -z "$history" && -z "$skipped" ]]; then
        echo "No prior refinement history for this focus area."
    else
        echo "${history}${skipped}"
    fi
}
```

**Step 4: Add `update_ledger` function**

```bash
# Update ledger after an iteration.
# Usage: update_ledger <action> <impact> <summary> [pr_number] [skip_reason] [type]
update_ledger() {
    local action="$1"
    local impact="$2"
    local summary="$3"
    local pr_number="${4:-}"
    local skip_reason="${5:-}"
    local finding_type="${6:-}"

    local area_key
    area_key="$(echo "$FOCUS_PATH" | sed 's/\./\\./g')"
    local now
    now="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    local today
    today="$(date '+%Y-%m-%d')"

    # Initialize ledger if it doesn't exist
    if [[ ! -f "$LEDGER" ]]; then
        cat > "$LEDGER" << INIT
# Auto-maintained by refine.sh — manual edits are fine for overrides
INIT
    fi

    # Initialize area if it doesn't exist
    local area_status
    area_status="$(yq -p toml -oy ".areas.\"$area_key\".status" "$LEDGER" 2>/dev/null || echo "null")"
    if [[ "$area_status" == "null" ]]; then
        yq -i -p toml -o toml ".areas.\"$area_key\".status = \"active\"" "$LEDGER"
        yq -i -p toml -o toml ".areas.\"$area_key\".total_prs = 0" "$LEDGER"
        yq -i -p toml -o toml ".areas.\"$area_key\".total_skips = 0" "$LEDGER"
        yq -i -p toml -o toml ".areas.\"$area_key\".consecutive_idle = 0" "$LEDGER"
        yq -i -p toml -o toml ".areas.\"$area_key\".last_run = \"$now\"" "$LEDGER"
        yq -i -p toml -o toml ".areas.\"$area_key\".graduated_at = \"\"" "$LEDGER"
    fi

    # Update last_run
    yq -i -p toml -o toml ".areas.\"$area_key\".last_run = \"$now\"" "$LEDGER"

    case "$action" in
        change)
            yq -i -p toml -o toml ".areas.\"$area_key\".consecutive_idle = 0" "$LEDGER"
            local cur_prs
            cur_prs="$(yq -p toml -oy ".areas.\"$area_key\".total_prs" "$LEDGER")"
            yq -i -p toml -o toml ".areas.\"$area_key\".total_prs = $((cur_prs + 1))" "$LEDGER"
            # Append history entry
            yq -i -p toml -o toml \
                ".areas.\"$area_key\".history += [{\"pr\": ${pr_number:-0}, \"type\": \"${finding_type}\", \"impact\": \"${impact}\", \"summary\": \"${summary}\"}]" \
                "$LEDGER"
            ;;
        skip)
            local cur_idle cur_skips
            cur_idle="$(yq -p toml -oy ".areas.\"$area_key\".consecutive_idle" "$LEDGER")"
            cur_skips="$(yq -p toml -oy ".areas.\"$area_key\".total_skips" "$LEDGER")"
            yq -i -p toml -o toml ".areas.\"$area_key\".consecutive_idle = $((cur_idle + 1))" "$LEDGER"
            yq -i -p toml -o toml ".areas.\"$area_key\".total_skips = $((cur_skips + 1))" "$LEDGER"
            # Append skipped entry
            yq -i -p toml -o toml \
                ".areas.\"$area_key\".skipped += [{\"type\": \"${finding_type}\", \"summary\": \"${summary}\", \"reason\": \"${skip_reason}\", \"date\": \"${today}\"}]" \
                "$LEDGER"
            ;;
        clean)
            local cur_idle
            cur_idle="$(yq -p toml -oy ".areas.\"$area_key\".consecutive_idle" "$LEDGER")"
            yq -i -p toml -o toml ".areas.\"$area_key\".consecutive_idle = $((cur_idle + 1))" "$LEDGER"
            ;;
        ticket)
            yq -i -p toml -o toml ".areas.\"$area_key\".consecutive_idle = 0" "$LEDGER"
            ;;
    esac

    log "Ledger updated: action=$action impact=$impact"
}

# Mark focus area as graduated in the ledger
graduate_area() {
    local area_key
    area_key="$(echo "$FOCUS_PATH" | sed 's/\./\\./g')"
    local now
    now="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    yq -i -p toml -o toml ".areas.\"$area_key\".status = \"graduated\"" "$LEDGER"
    yq -i -p toml -o toml ".areas.\"$area_key\".graduated_at = \"$now\"" "$LEDGER"
    log "Area $FOCUS_PATH graduated"
}

# Commit ledger changes to master
commit_ledger() {
    if [[ ! -f "$LEDGER" ]]; then
        return 0
    fi
    # Only commit if ledger has changes
    if git -C "$REPO_ROOT" diff --quiet "$LEDGER" 2>/dev/null && \
       ! git -C "$REPO_ROOT" ls-files --others --exclude-standard | grep -q "refine-ledger.toml"; then
        return 0
    fi
    git -C "$REPO_ROOT" add "$LEDGER"
    git -C "$REPO_ROOT" commit --quiet -m "refine: update ledger"
    git -C "$REPO_ROOT" push --quiet origin master
    log "Ledger committed and pushed"
}
```

**Step 5: Commit**

```bash
git add scripts/refine.sh
git commit -m "refine: add ledger read/write and impact threshold functions"
```

---

### Task 4: Wire `{{LEDGER_CONTEXT}}` into prompt builder

**Files:**
- Modify: `scripts/refine.sh` (the `build_prompt` function, ~line 104-155)

**Step 1: Update `build_prompt` to accept ledger context**

Change the function signature and add ledger context substitution. The function currently takes `pending_changes` as arg 1. Add `ledger_context` as arg 2:

```bash
build_prompt() {
    local pending_changes="${1:-}"
    local ledger_context="${2:-No prior refinement history for this focus area.}"
```

Add `LEDGER_CONTEXT` to the perl substitution block (after the `PENDING_CHANGES` line in the env vars and the perl substitution):

Add to the environment variables before `perl`:
```bash
    LEDGER_CONTEXT="$ledger_context" \
```

Add to the perl substitution:
```perl
        s/\Q{{LEDGER_CONTEXT}}\E/$ENV{LEDGER_CONTEXT}/g;
```

**Step 2: Update `run_iteration` to pass ledger context**

In `run_iteration`, after `pending_summary="$(get_pending_pr_summary)"` (~line 485), add:

```bash
    # Read ledger context for the prompt
    local ledger_context
    ledger_context="$(read_ledger_context)"
```

Change the `build_prompt` call from:
```bash
    prompt="$(build_prompt "$pending_summary")"
```
to:
```bash
    prompt="$(build_prompt "$pending_summary" "$ledger_context")"
```

**Step 3: Verify with dry-run**

```bash
./scripts/refine.sh --dry-run 2>&1 | grep -A5 "History for this"
```

Expected: "No prior refinement history for this focus area."

**Step 4: Commit**

```bash
git add scripts/refine.sh
git commit -m "refine: wire ledger context into prompt builder"
```

---

### Task 5: Handle `skip` action and impact threshold in iteration dispatch

**Files:**
- Modify: `scripts/refine.sh` (in `run_iteration`, ~line 580-607)

**Step 1: Extract impact from JSON and check threshold**

After `summary="$(echo..."` (~line 584), add:

```bash
    local impact
    impact="$(echo "$json_block" | jq -r '.impact // "medium"' 2>/dev/null || echo "medium")"
    local skip_reason
    skip_reason="$(echo "$json_block" | jq -r '.skip_reason // ""' 2>/dev/null || echo "")"

    # Downgrade change to skip if below impact threshold
    if [[ "$action" == "change" ]]; then
        local impact_num
        impact_num="$(impact_to_num "$impact")"
        if [[ "$impact_num" -lt "$IMPACT_THRESHOLD" ]]; then
            log "Downgrading change to skip: impact=$impact below threshold=$MIN_IMPACT"
            action="skip"
            skip_reason="impact $impact below min_impact $MIN_IMPACT"
            # Undo the change — clean up the worktree without pushing
            cleanup_worktree "$wt_path" "$branch"
        fi
    fi
```

**Step 2: Update the action dispatch case statement**

Replace the existing case statement (~line 588-605) with:

```bash
    case "$action" in
        change)
            handle_change "$wt_path" "$branch" "$summary"
            # Extract PR number from the most recent PR for ledger
            local pr_num
            pr_num="$(gh pr list --head "$branch" --json number --jq '.[0].number' 2>/dev/null || echo "0")"
            update_ledger "change" "$impact" "$summary" "$pr_num" "" ""
            ;;
        skip)
            log "Skipped: $summary ($skip_reason)"
            cleanup_worktree "$wt_path" "$branch"
            update_ledger "skip" "$impact" "$summary" "" "$skip_reason" ""
            ;;
        ticket)
            handle_ticket "$json_block" "$summary"
            cleanup_worktree "$wt_path" "$branch"
            update_ledger "ticket" "$impact" "$summary"
            ;;
        clean)
            log "Focus area clean: $summary"
            cleanup_worktree "$wt_path" "$branch"
            update_ledger "clean" "$impact" "$summary"
            ;;
        *)
            log "WARNING: Unknown action '$action', treating as error"
            cleanup_worktree "$wt_path" "$branch"
            return 1
            ;;
    esac

    # Commit ledger after each iteration
    commit_ledger

    echo "$action"
```

**Step 3: Commit**

```bash
git add scripts/refine.sh
git commit -m "refine: handle skip action and impact threshold in iteration dispatch"
```

---

### Task 6: Update main loop stopping logic

**Files:**
- Modify: `scripts/refine.sh` (the `main` function, ~line 612-670)

**Step 1: Update the case statement in the main loop**

Replace the case statement (~line 637-658) with:

```bash
        case "$action" in
            change)
                idle_count=0
                pr_count=$((pr_count + 1))
                log "PRs opened so far: $pr_count"
                if [[ "$MAX_PRS" -gt 0 && "$pr_count" -ge "$MAX_PRS" ]]; then
                    log "Reached max_prs=$MAX_PRS, stopping"
                    break
                fi
                ;;
            ticket)
                idle_count=0
                ;;
            clean|skip)
                idle_count=$((idle_count + 1))
                log "Idle count: $idle_count / $IDLE_LIMIT"
                if [[ "$idle_count" -ge "$IDLE_LIMIT" ]]; then
                    log "Reached idle_limit=$IDLE_LIMIT, focus area graduated"
                    graduate_area
                    commit_ledger
                    break
                fi
                ;;
        esac
```

**Step 2: Add graduated area check at loop start**

At the beginning of `main()`, after reading config, add a check for graduated areas:

```bash
    # Check if focus area is graduated
    if [[ -f "$LEDGER" ]]; then
        local area_key
        area_key="$(echo "$FOCUS_PATH" | sed 's/\./\\./g')"
        local area_status
        area_status="$(yq -p toml -oy ".areas.\"$area_key\".status" "$LEDGER" 2>/dev/null || echo "null")"
        if [[ "$area_status" == "graduated" ]]; then
            log "Focus area $FOCUS_PATH is graduated. Set status to 'active' in refine-ledger.toml to re-run."
            exit 0
        fi
    fi
```

**Step 3: Verify with dry-run**

```bash
./scripts/refine.sh --dry-run 2>&1
```

Expected: Renders prompt, no errors. Graduated check passes (no ledger yet).

**Step 4: Commit**

```bash
git add scripts/refine.sh
git commit -m "refine: skip counts as idle, graduate area on idle_limit"
```

---

### Task 7: Seed initial ledger from existing PR history

**Files:**
- Create: `scripts/seed-ledger.sh` (one-time migration script)

**Step 1: Write the seeding script**

```bash
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
```

**Step 2: Run it**

```bash
chmod +x scripts/seed-ledger.sh
./scripts/seed-ledger.sh
```

**Step 3: Review the generated ledger**

```bash
cat refine-ledger.toml
```

Verify it looks reasonable — 27 entries with inferred types.

**Step 4: Commit**

```bash
git add scripts/seed-ledger.sh refine-ledger.toml
git commit -m "refine: seed ledger from existing 27 merged refinement PRs"
```

---

### Task 8: End-to-end dry-run verification

**Files:** None (verification only)

**Step 1: Run full dry-run**

```bash
./scripts/refine.sh --dry-run 2>&1
```

Expected:
- Config loads with `min_impact=medium`
- Ledger context includes seeded history
- Prompt includes `skip` action in schema
- Prompt includes impact assessment rule
- No template variables remain unexpanded

**Step 2: Verify graduated area blocking**

```bash
# Temporarily graduate the area
yq -i -p toml -o toml '.areas."service/src/identity/".status = "graduated"' refine-ledger.toml

./scripts/refine.sh --dry-run 2>&1

# Expected: "Focus area service/src/identity/ is graduated..."
# Reset
yq -i -p toml -o toml '.areas."service/src/identity/".status = "active"' refine-ledger.toml
```

**Step 3: Commit any fixes if needed, then final commit**

```bash
git add -A
git commit -m "refine: end-to-end verification of impact scoring and ledger"
```

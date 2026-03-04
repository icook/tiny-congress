#!/usr/bin/env bash
set -euo pipefail

# Automated refinement coordinator
# Usage: ./scripts/refine.sh [--dry-run]

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CONFIG="$REPO_ROOT/refine.toml"
LOG_DIR="$REPO_ROOT/.claude"
LOG_FILE="$LOG_DIR/refine.log"
DRY_RUN=false

[[ "${1:-}" == "--dry-run" ]] && DRY_RUN=true

# ── Config parsing ──────────────────────────────────────────────────
read_config() {
    yq -p toml -oy "$1" "$CONFIG"
}

require_config() {
    local value
    value="$(read_config "$1")"
    if [[ -z "$value" || "$value" == "null" ]]; then
        echo "ERROR: required config key '$1' is missing or empty in $CONFIG" >&2
        exit 1
    fi
    echo "$value"
}

FOCUS_PATH="$(require_config '.focus.path')"
GUIDANCE_FILE="$(require_config '.prompts.guidance')"
COOLDOWN="$(require_config '.behavior.cooldown')"
MAX_PRS="$(require_config '.behavior.max_prs')"
IDLE_LIMIT="$(require_config '.behavior.idle_limit')"

MIN_IMPACT="$(read_config '.behavior.min_impact')"
[[ -z "$MIN_IMPACT" || "$MIN_IMPACT" == "null" ]] && MIN_IMPACT="medium"

LEDGER="$REPO_ROOT/refine-ledger.json"

# CI auto-fix config (optional — defaults to disabled)
CI_AUTO_FIX="$(read_config '.ci.auto_fix')"
CI_MAX_FIX_ATTEMPTS="$(read_config '.ci.max_fix_attempts')"
CI_CHECK_TIMEOUT="$(read_config '.ci.check_timeout')"
[[ "$CI_AUTO_FIX" != "true" ]] && CI_AUTO_FIX=false
[[ -z "$CI_MAX_FIX_ATTEMPTS" || "$CI_MAX_FIX_ATTEMPTS" == "null" ]] && CI_MAX_FIX_ATTEMPTS=2
[[ -z "$CI_CHECK_TIMEOUT" || "$CI_CHECK_TIMEOUT" == "null" ]] && CI_CHECK_TIMEOUT=900

# Parse enabled types into a list (priority order: security > patterns > tests > cleanup)
ENABLED_TYPES=()
[[ "$(read_config '.types.security_hardening')" == "true" ]] && ENABLED_TYPES+=("security_hardening")
[[ "$(read_config '.types.pattern_enforcement')" == "true" ]] && ENABLED_TYPES+=("pattern_enforcement")
[[ "$(read_config '.types.test_coverage')" == "true" ]] && ENABLED_TYPES+=("test_coverage")
[[ "$(read_config '.types.code_cleanup')" == "true" ]] && ENABLED_TYPES+=("code_cleanup")

mkdir -p "$LOG_DIR"

log() {
    local msg
    msg="[$(date '+%Y-%m-%d %H:%M:%S')] $*"
    echo "$msg"
    echo "$msg" >> "$LOG_FILE"
}

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

log "Config loaded: focus=$FOCUS_PATH types=${ENABLED_TYPES[*]} idle_limit=$IDLE_LIMIT"

# ── Prompt templating ───────────────────────────────────────────────

# Map type names to prompt sections
type_section() {
    case "$1" in
        security_hardening)
            cat <<'SECTION'
### Security Hardening
- Missing input validation at module boundaries
- Unchecked assumptions about data format or size
- Error cases that silently succeed
- String comparison used for secret-adjacent values
SECTION
            ;;
        pattern_enforcement)
            cat <<'SECTION'
### Pattern Enforcement
- String parameters that should be newtypes
- Error handling that doesn't match project conventions (thiserror vs anyhow)
- Duplicated logic that should be consolidated into a single code path
SECTION
            ;;
        test_coverage)
            cat <<'SECTION'
### Test Coverage
- Untested error paths (functions returning Result without Err tests)
- Missing boundary input tests (empty, zero, max-length)
- Tests that don't assert meaningful behavior
SECTION
            ;;
        code_cleanup)
            cat <<'SECTION'
### Code Cleanup
- Dead code, unused imports, unreachable branches
- Overly complex expressions that can be simplified
- TODO/FIXME items that are now actionable
SECTION
            ;;
    esac
}

build_prompt() {
    local pending_changes="${1:-}"
    local ledger_context="${2:-No prior refinement history for this focus area.}"
    local template_file="$SCRIPT_DIR/refine-prompt.md"
    if [[ ! -f "$template_file" ]]; then
        log "ERROR: Prompt template not found at $template_file"
        exit 1
    fi
    # Read guidance content
    local guidance=""
    local guidance_path="$REPO_ROOT/$GUIDANCE_FILE"
    if [[ -f "$guidance_path" ]]; then
        guidance="$(cat "$guidance_path")"
    else
        log "WARNING: Guidance file not found at $guidance_path"
    fi

    # Build enabled types section
    local types_section=""
    for t in "${ENABLED_TYPES[@]}"; do
        types_section+="$(type_section "$t")"$'\n'
    done

    # Build pending changes section
    local pending_section=""
    if [[ -n "$pending_changes" ]]; then
        pending_section="The following improvements are already in-progress (open PRs). Do NOT re-implement these:

$pending_changes
Find a DIFFERENT improvement instead."
    else
        pending_section="No pending refinement PRs."
    fi

    # Use perl for safe substitution — bash ${//} treats & as backreference
    # in newer versions, corrupting content like TryFrom<&str>
    FOCUS_PATH="$FOCUS_PATH" \
    GUIDANCE_CONTENT="$guidance" \
    ENABLED_TYPES="$types_section" \
    PENDING_CHANGES="$pending_section" \
    LEDGER_CONTEXT="$ledger_context" \
    perl -0777 -p -e '
        s/\Q{{FOCUS_PATH}}\E/$ENV{FOCUS_PATH}/g;
        s/\Q{{GUIDANCE_CONTENT}}\E/$ENV{GUIDANCE_CONTENT}/g;
        s/\Q{{ENABLED_TYPES}}\E/$ENV{ENABLED_TYPES}/g;
        s/\Q{{PENDING_CHANGES}}\E/$ENV{PENDING_CHANGES}/g;
        s/\Q{{LEDGER_CONTEXT}}\E/$ENV{LEDGER_CONTEXT}/g;
    ' "$template_file"
}

# ── Ledger read/write ─────────────────────────────────────────────

# Produce a markdown summary of ledger history for the current focus area.
# Used to give Claude context about what has already been done/skipped.
read_ledger_context() {
    if [[ ! -f "$LEDGER" ]]; then
        echo "No prior refinement history for this focus area."
        return 0
    fi

    local area_key="$FOCUS_PATH"
    local status
    status="$(jq -r --arg key "$area_key" '.areas[$key].status // empty' "$LEDGER" 2>/dev/null || echo "")"

    if [[ -z "$status" || "$status" == "null" ]]; then
        echo "No prior refinement history for this focus area."
        return 0
    fi

    local total_prs total_skips consecutive_idle last_run
    total_prs="$(jq -r --arg key "$area_key" '.areas[$key].total_prs // 0' "$LEDGER" 2>/dev/null || echo "0")"
    total_skips="$(jq -r --arg key "$area_key" '.areas[$key].total_skips // 0' "$LEDGER" 2>/dev/null || echo "0")"
    consecutive_idle="$(jq -r --arg key "$area_key" '.areas[$key].consecutive_idle // 0' "$LEDGER" 2>/dev/null || echo "0")"
    last_run="$(jq -r --arg key "$area_key" '.areas[$key].last_run // ""' "$LEDGER" 2>/dev/null || echo "")"

    local context="## Refinement Ledger — \`${area_key}\`

**Status:** ${status} | **PRs:** ${total_prs} | **Skips:** ${total_skips} | **Consecutive idle:** ${consecutive_idle} | **Last run:** ${last_run}

### Recent history (last 20)
"

    local history_count
    history_count="$(jq -r --arg key "$area_key" '.areas[$key].history | length' "$LEDGER" 2>/dev/null || echo "0")"

    if [[ "$history_count" -gt 0 ]]; then
        local start=0
        if [[ "$history_count" -gt 20 ]]; then
            start=$((history_count - 20))
        fi
        local i
        for ((i = start; i < history_count; i++)); do
            local pr type impact summary
            pr="$(jq -r --arg key "$area_key" --argjson i "$i" '.areas[$key].history[$i].pr // ""' "$LEDGER" 2>/dev/null || echo "")"
            type="$(jq -r --arg key "$area_key" --argjson i "$i" '.areas[$key].history[$i].type // ""' "$LEDGER" 2>/dev/null || echo "")"
            impact="$(jq -r --arg key "$area_key" --argjson i "$i" '.areas[$key].history[$i].impact // ""' "$LEDGER" 2>/dev/null || echo "")"
            summary="$(jq -r --arg key "$area_key" --argjson i "$i" '.areas[$key].history[$i].summary // ""' "$LEDGER" 2>/dev/null || echo "")"
            context+="- PR #${pr} [${type}/${impact}]: ${summary}"$'\n'
        done
    else
        context+="_No history entries yet._"$'\n'
    fi

    context+=$'\n'"### Skipped items"$'\n'

    local skipped_count
    skipped_count="$(jq -r --arg key "$area_key" '.areas[$key].skipped | length' "$LEDGER" 2>/dev/null || echo "0")"

    if [[ "$skipped_count" -gt 0 ]]; then
        local i
        for ((i = 0; i < skipped_count; i++)); do
            local type summary reason date
            type="$(jq -r --arg key "$area_key" --argjson i "$i" '.areas[$key].skipped[$i].type // ""' "$LEDGER" 2>/dev/null || echo "")"
            summary="$(jq -r --arg key "$area_key" --argjson i "$i" '.areas[$key].skipped[$i].summary // ""' "$LEDGER" 2>/dev/null || echo "")"
            reason="$(jq -r --arg key "$area_key" --argjson i "$i" '.areas[$key].skipped[$i].reason // ""' "$LEDGER" 2>/dev/null || echo "")"
            date="$(jq -r --arg key "$area_key" --argjson i "$i" '.areas[$key].skipped[$i].date // ""' "$LEDGER" 2>/dev/null || echo "")"
            context+="- [${type}] ${summary} — _${reason}_ (${date})"$'\n'
        done
    else
        context+="_No skipped items._"$'\n'
    fi

    echo "$context"
}

# Update the ledger after an iteration.
# Usage: update_ledger <action> <impact> <summary> [pr_number] [skip_reason] [finding_type]
update_ledger() {
    local action="$1"
    local impact="$2"
    local summary="$3"
    local pr_number="${4:-}"
    local skip_reason="${5:-}"
    local finding_type="${6:-}"

    local area_key="$FOCUS_PATH"
    local now
    now="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    local today
    today="$(date -u '+%Y-%m-%d')"

    # Initialize the area in the ledger if it doesn't exist
    local area_status=""
    if [[ -f "$LEDGER" ]]; then
        area_status="$(jq -r --arg key "$area_key" '.areas[$key].status // empty' "$LEDGER" 2>/dev/null || echo "")"
    fi

    if [[ ! -f "$LEDGER" ]] || [[ -z "$area_status" ]]; then
        if [[ ! -f "$LEDGER" ]]; then
            local tmp
            tmp="$(jq -n --arg key "$area_key" '{areas: {($key): {status: "active", total_prs: 0, total_skips: 0, consecutive_idle: 0, last_run: "", graduated_at: "", history: [], skipped: []}}}')"
            echo "$tmp" > "$LEDGER"
        else
            local tmp
            tmp="$(jq --arg key "$area_key" '.areas[$key] = {status: "active", total_prs: 0, total_skips: 0, consecutive_idle: 0, last_run: "", graduated_at: "", history: [], skipped: []}' "$LEDGER")"
            echo "$tmp" > "$LEDGER"
        fi
    fi

    # Update last_run timestamp
    local tmp
    tmp="$(jq --arg key "$area_key" --arg now "$now" '.areas[$key].last_run = $now' "$LEDGER")"
    echo "$tmp" > "$LEDGER"

    case "$action" in
        change)
            # Increment total_prs, reset consecutive_idle, append to history
            local tmp
            tmp="$(jq --arg key "$area_key" \
                --argjson pr "${pr_number:-0}" \
                --arg type "$finding_type" \
                --arg impact "$impact" \
                --arg summary "$summary" \
                '.areas[$key].total_prs += 1 |
                 .areas[$key].consecutive_idle = 0 |
                 .areas[$key].history += [{pr: $pr, type: $type, impact: $impact, summary: $summary}]' \
                "$LEDGER")"
            echo "$tmp" > "$LEDGER"
            ;;
        skip)
            # Increment total_skips, append to skipped
            local tmp
            tmp="$(jq --arg key "$area_key" \
                --arg type "$finding_type" \
                --arg summary "$summary" \
                --arg reason "$skip_reason" \
                --arg date "$today" \
                '.areas[$key].total_skips += 1 |
                 .areas[$key].skipped += [{type: $type, summary: $summary, reason: $reason, date: $date}]' \
                "$LEDGER")"
            echo "$tmp" > "$LEDGER"
            ;;
        clean)
            # Increment consecutive_idle
            local tmp
            tmp="$(jq --arg key "$area_key" '.areas[$key].consecutive_idle += 1' "$LEDGER")"
            echo "$tmp" > "$LEDGER"
            ;;
    esac
}

# Mark an area as graduated (fully refined).
graduate_area() {
    local area_key="$FOCUS_PATH"
    local now
    now="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

    local tmp
    tmp="$(jq --arg key "$area_key" --arg now "$now" \
        '.areas[$key].status = "graduated" | .areas[$key].graduated_at = $now' \
        "$LEDGER")"
    echo "$tmp" > "$LEDGER"
    log "Area '$area_key' graduated at $now"
}

# Commit and push ledger changes to master if there are any.
commit_ledger() {
    if [[ ! -f "$LEDGER" ]]; then
        return 0
    fi

    # Check if ledger has actual changes
    if ! git -C "$REPO_ROOT" diff --quiet -- "refine-ledger.json" 2>/dev/null || \
       ! git -C "$REPO_ROOT" diff --cached --quiet -- "refine-ledger.json" 2>/dev/null || \
       [[ -n "$(git -C "$REPO_ROOT" ls-files --others --exclude-standard -- "refine-ledger.json" 2>/dev/null)" ]]; then
        git -C "$REPO_ROOT" add "refine-ledger.json"
        git -C "$REPO_ROOT" commit --quiet -m "refine: update ledger for ${FOCUS_PATH}"
        git -C "$REPO_ROOT" push --quiet origin master
        log "Ledger committed and pushed to master"
    fi
}

# ── Pending PR deduplication ───────────────────────────────────────

# Gather open refinement PRs as a markdown list for inclusion in the prompt.
# Claude sees the PR titles and avoids rediscovering the same issues.
# Titles are sanitized (whitespace collapsed, truncated) to prevent prompt injection.
get_pending_pr_summary() {
    local prs
    local gh_exit=0
    prs="$(gh pr list --label "refinement" --state open \
        --json number,title \
        --jq '.[] | "- #\(.number): \(.title)"' 2>/dev/null)" || gh_exit=$?

    if [[ $gh_exit -ne 0 ]]; then
        log "WARNING: gh pr list failed (exit $gh_exit), skipping deduplication"
        echo ""
        return 0
    fi

    if [[ -z "$prs" ]]; then
        echo ""
        return 0
    fi

    # Sanitize: collapse whitespace and cap each title to 120 chars
    prs="$(echo "$prs" | sed 's/[[:space:]]\{1,\}/ /g' | cut -c1-120)"

    log "Found $(echo "$prs" | wc -l | tr -d ' ') open refinement PR(s)"
    echo "$prs"
}

if $DRY_RUN; then
    # Check graduated status even in dry-run
    if [[ -f "$LEDGER" ]]; then
        local_status="$(jq -r --arg key "$FOCUS_PATH" '.areas[$key].status // "null"' "$LEDGER" 2>/dev/null || echo "null")"
        if [[ "$local_status" == "graduated" ]]; then
            log "Focus area $FOCUS_PATH is graduated. Set status to 'active' in refine-ledger.json to re-run."
            exit 0
        fi
    fi
    log "=== DRY RUN: Generated prompt ==="
    build_prompt "$(get_pending_pr_summary)" "$(read_ledger_context)"
    exit 0
fi

# ── Worktree management ────────────────────────────────────────────

WORKTREE_DIR="$REPO_ROOT/.claude/worktrees"

cleanup_worktree() {
    local wt_path="$1"
    local branch="$2"
    git -C "$REPO_ROOT" worktree remove "$wt_path" --force 2>/dev/null || true
    git -C "$REPO_ROOT" branch -D "$branch" 2>/dev/null || true
    log "Cleaned up worktree $wt_path"
}

# ── CI auto-fix ───────────────────────────────────────────────────

# Wait for CI checks on a PR. Returns 0 if all pass, 1 if any fail, 2 on timeout.
wait_for_ci() {
    local pr_url="$1"
    local timeout="$CI_CHECK_TIMEOUT"
    local interval=30
    local elapsed=0

    log "Waiting for CI checks on $pr_url (timeout ${timeout}s)"

    while [[ $elapsed -lt $timeout ]]; do
        # gh pr checks returns uppercase states: PENDING, QUEUED, IN_PROGRESS, SUCCESS, FAILURE, etc.
        # Verified empirically — `gh pr checks <url> --json state --jq '.[].state'`
        local states
        states="$(gh pr checks "$pr_url" --json state --jq '[.[].state] | unique | join(",")' 2>/dev/null || echo "UNKNOWN")"

        # If no checks exist yet, wait
        if [[ "$states" == "UNKNOWN" || -z "$states" ]]; then
            sleep "$interval"
            elapsed=$((elapsed + interval))
            continue
        fi

        # All passed
        if [[ "$states" == "SUCCESS" ]]; then
            log "CI passed for $pr_url"
            return 0
        fi

        # Any failure/error/cancelled (and no pending)
        if [[ "$states" != *"PENDING"* && "$states" != *"QUEUED"* && "$states" != *"IN_PROGRESS"* ]]; then
            log "CI failed for $pr_url (states: $states)"
            return 1
        fi

        sleep "$interval"
        elapsed=$((elapsed + interval))
    done

    log "WARNING: CI check timeout after ${timeout}s for $pr_url"
    return 2
}

# Attempt to fix a CI failure on a PR branch.
# Checks out the branch, gets failure logs, runs Claude to fix, pushes.
fix_ci_failure() {
    local pr_url="$1"
    local branch="$2"
    local attempt="$3"

    log "Attempting CI fix $attempt/$CI_MAX_FIX_ATTEMPTS for $pr_url"

    # Get the failed workflow run
    local run_id
    run_id="$(gh run list --branch "$branch" --limit 1 --status failure \
        --json databaseId --jq '.[0].databaseId' 2>/dev/null || echo "")"

    if [[ -z "$run_id" ]]; then
        log "WARNING: Could not find failed run for branch $branch"
        return 1
    fi

    # Get failure logs (last 200 lines)
    local failure_logs
    failure_logs="$(gh run view "$run_id" --log-failed 2>/dev/null | tail -200)"

    if [[ -z "$failure_logs" ]]; then
        log "WARNING: Could not fetch failure logs for run $run_id"
        return 1
    fi

    # Create a worktree from the PR branch
    local fix_id
    fix_id="fix-ci-$(date '+%Y%m%d-%H%M%S')"
    local wt_path="$WORKTREE_DIR/$fix_id"

    git -C "$REPO_ROOT" fetch --quiet origin "$branch"
    git -C "$REPO_ROOT" worktree add "$wt_path" "origin/$branch" --detach --quiet
    # Re-attach to the branch so we can push
    git -C "$wt_path" checkout "$branch" --quiet 2>/dev/null || \
        git -C "$wt_path" checkout -b "$branch" --quiet

    local fix_prompt
    fix_prompt="Fix the CI failure on this branch.

## Failed CI logs (last 200 lines)

\`\`\`
$failure_logs
\`\`\`

## Rules

1. Fix ONLY the CI failure — do not refactor or improve other things.
2. Run the failing command locally to verify the fix works.
3. Commit with a message starting with \`fix(ci):\`.
4. If the failure is a flaky test or infrastructure issue (not a code problem), output: {\"action\": \"skip\", \"reason\": \"description of why\"}"

    local claude_output
    local claude_exit=0
    claude_output="$(
        cd "$wt_path" && \
        unset CLAUDECODE && \
        claude -p \
            --output-format json \
            --allowedTools "Read Edit Write Bash Glob Grep" \
            --no-session-persistence \
            --max-turns 30 \
            "$fix_prompt"
    )" || claude_exit=$?

    if [[ $claude_exit -ne 0 ]]; then
        log "ERROR: Claude CI fix exited with code $claude_exit"
        git -C "$REPO_ROOT" worktree remove "$wt_path" --force 2>/dev/null || true
        return 1
    fi

    # Check if Claude signalled a skip (flaky test / infra issue)
    local action
    action="$(echo "$claude_output" | jq -r '.result // ""' 2>/dev/null | jq -r '.action // ""' 2>/dev/null || echo "")"
    if [[ "$action" == "skip" ]]; then
        local reason
        reason="$(echo "$claude_output" | jq -r '.result // ""' 2>/dev/null | jq -r '.reason // "no reason given"' 2>/dev/null || echo "no reason given")"
        log "CI fix skipped: $reason"
        git -C "$REPO_ROOT" worktree remove "$wt_path" --force 2>/dev/null || true
        return 1
    fi

    # Check if Claude made any commits
    local new_commits
    new_commits="$(git -C "$wt_path" rev-list --count "origin/$branch..HEAD" 2>/dev/null || echo "0")"

    if [[ "$new_commits" -eq 0 ]]; then
        log "WARNING: Claude CI fix made no commits"
        git -C "$REPO_ROOT" worktree remove "$wt_path" --force 2>/dev/null || true
        return 1
    fi

    # Push the fix
    git -C "$wt_path" push --quiet origin "HEAD:$branch"
    log "Pushed CI fix ($new_commits commit(s)) to $branch"

    git -C "$REPO_ROOT" worktree remove "$wt_path" --force 2>/dev/null || true
    return 0
}

# ── Handlers ────────────────────────────────────────────────────────

handle_change() {
    local wt_path="$1"
    local branch="$2"
    local summary="$3"

    # Ensure cleanup always runs, even if push/PR creation fails
    trap 'cleanup_worktree "$wt_path" "$branch"' RETURN

    # Verify there are actual commits beyond master
    local commit_count
    commit_count="$(git -C "$wt_path" rev-list --count master..HEAD)"
    if [[ "$commit_count" -eq 0 ]]; then
        log "WARNING: Claude reported 'change' but no commits found"
        return 1
    fi

    # Push branch
    git -C "$wt_path" push --quiet origin "$branch"
    log "Pushed $branch"

    # Open PR
    local pr_title
    pr_title="$(git -C "$wt_path" log -1 --format=%s)"
    local pr_body
    pr_body="Automated refinement of \`$FOCUS_PATH\`

$summary

---
*Generated by [refine.sh](scripts/refine.sh)*"

    local pr_url
    pr_url="$(gh pr create \
        --repo "$(gh repo view --json nameWithOwner -q .nameWithOwner)" \
        --head "$branch" \
        --title "$pr_title" \
        --body "$pr_body" \
        --label "refinement" --label "auto-merge")"
    log "Created PR: $pr_url"

    # Enable auto-merge — don't fail the iteration if this fails
    if gh pr merge "$pr_url" --auto --squash; then
        log "Auto-merge enabled for $pr_url"
    else
        log "WARNING: Could not enable auto-merge for $pr_url (may need manual merge)"
    fi

    # CI auto-fix: wait for checks and attempt to fix failures
    if $CI_AUTO_FIX; then
        local fix_attempt=0
        while [[ $fix_attempt -lt $CI_MAX_FIX_ATTEMPTS ]]; do
            local ci_result=0
            wait_for_ci "$pr_url" || ci_result=$?

            case $ci_result in
                0)  # CI passed
                    break
                    ;;
                1)  # CI failed — attempt fix
                    fix_attempt=$((fix_attempt + 1))
                    if fix_ci_failure "$pr_url" "$branch" "$fix_attempt"; then
                        log "CI fix pushed, waiting for new checks"
                    else
                        log "WARNING: CI fix attempt $fix_attempt failed"
                        break
                    fi
                    ;;
                *)  # Timeout or unknown — don't block the loop
                    log "WARNING: CI check inconclusive, moving on"
                    break
                    ;;
            esac
        done

        if [[ $fix_attempt -ge $CI_MAX_FIX_ATTEMPTS ]]; then
            log "WARNING: Exhausted $CI_MAX_FIX_ATTEMPTS CI fix attempts for $pr_url"
        fi
    fi
}

handle_ticket() {
    local parsed="$1"
    local summary="$2"

    local title
    title="$(echo "$parsed" | jq -r '.ticket_title // empty' 2>/dev/null || echo "")"
    local body
    body="$(echo "$parsed" | jq -r '.ticket_body // empty' 2>/dev/null || echo "")"

    if [[ -z "$title" ]]; then
        title="refine: $summary"
    fi
    if [[ -z "$body" ]]; then
        body="$summary"
    fi

    # Deduplicate — check for existing issues with same title
    local existing
    existing="$(gh issue list --label "refinement" --label "needs-design" \
        --search "in:title $title" --json number --jq '.[0].number' 2>/dev/null || echo "")"

    if [[ -n "$existing" ]]; then
        log "Duplicate finding, existing issue #$existing"
        return 0
    fi

    local issue_url
    issue_url="$(gh issue create \
        --title "$title" \
        --body "$body" \
        --label "refinement" --label "needs-design")"
    log "Created issue: $issue_url"
}

# ── Single refinement iteration ─────────────────────────────────────

run_iteration() {
    local iteration_id
    iteration_id="$(date '+%Y%m%d-%H%M%S')"
    local slug
    slug="$(echo "$FOCUS_PATH" | tr '/' '-' | sed 's/-$//')"
    local branch="refine/${slug}-${iteration_id}"
    local wt_path="$WORKTREE_DIR/refine-${iteration_id}"

    log "Starting iteration $iteration_id on $FOCUS_PATH"

    # Ensure we're on latest master
    git -C "$REPO_ROOT" checkout master --quiet
    git -C "$REPO_ROOT" pull --rebase --quiet

    # Create worktree
    mkdir -p "$WORKTREE_DIR"
    git -C "$REPO_ROOT" worktree add "$wt_path" -b "$branch" --quiet
    log "Created worktree at $wt_path (branch: $branch)"

    # Query open refinement PRs so Claude avoids rediscovering the same issues
    local pending_summary
    pending_summary="$(get_pending_pr_summary)"

    local ledger_context
    ledger_context="$(read_ledger_context)"

    # Build prompt (includes pending PR info so Claude avoids duplicates)
    local prompt
    prompt="$(build_prompt "$pending_summary" "$ledger_context")"

    # Run Claude Code in the worktree
    local claude_output
    local claude_exit=0
    claude_output="$(
        cd "$wt_path" && \
        unset CLAUDECODE && \
        claude -p \
            --output-format json \
            --allowedTools "Read Edit Write Bash Glob Grep" \
            --no-session-persistence \
            --max-turns 50 \
            "$prompt"
    )" || claude_exit=$?

    if [[ $claude_exit -ne 0 ]]; then
        log "ERROR: Claude exited with code $claude_exit"
        cleanup_worktree "$wt_path" "$branch"
        return 1
    fi

    # Parse Claude's JSON response
    # claude --output-format json wraps the response; extract the text content
    local response_text
    response_text="$(echo "$claude_output" | jq -r '
        if type == "array" then
            map(select(.type == "text") | .text) | join("")
        elif .result then .result
        elif .content then
            if (.content | type) == "array" then
                .content | map(select(.type == "text") | .text) | join("")
            else .content
            end
        else tostring
        end
    ' 2>/dev/null || echo "$claude_output")"

    # Extract JSON object from the response text
    # Try multiple strategies: direct parse, code fence extraction, text scanning
    local json_block=""

    # Strategy 1: response_text is already valid JSON with "action"
    if echo "$response_text" | jq -e '.action' &>/dev/null; then
        json_block="$(echo "$response_text" | jq -c '.')"
    fi

    # Strategy 2: JSON is inside a markdown code fence
    if [[ -z "$json_block" ]]; then
        json_block="$(echo "$response_text" | \
            awk '/^```json/{found=1;next} /^```/{found=0} found{print}' | \
            jq -c '.' 2>/dev/null || echo "")"
        # Verify it has an action field
        if [[ -n "$json_block" ]] && ! echo "$json_block" | jq -e '.action' &>/dev/null; then
            json_block=""
        fi
    fi

    # Strategy 3: scan text for a JSON object containing "action" using python
    if [[ -z "$json_block" ]]; then
        json_block="$(python3 -c "
import json, sys
text = sys.stdin.read()
# Find JSON objects by matching balanced braces
depth = 0
start = None
for i, c in enumerate(text):
    if c == '{':
        if depth == 0:
            start = i
        depth += 1
    elif c == '}':
        depth -= 1
        if depth == 0 and start is not None:
            try:
                obj = json.loads(text[start:i+1])
                if 'action' in obj:
                    print(json.dumps(obj))
                    sys.exit(0)
            except json.JSONDecodeError:
                pass
            start = None
" <<< "$response_text" 2>/dev/null || echo "")"
    fi

    if [[ -z "$json_block" ]]; then
        log "WARNING: Could not parse JSON from Claude's response"
        log "Response: $(echo "$response_text" | head -5)"
        cleanup_worktree "$wt_path" "$branch"
        return 1
    fi

    local action
    action="$(echo "$json_block" | jq -r '.action' 2>/dev/null || echo "unknown")"
    local summary
    summary="$(echo "$json_block" | jq -r '.summary // "no summary"' 2>/dev/null || echo "no summary")"

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
        fi
    fi

    log "Result: action=$action summary=$summary"

    case "$action" in
        change)
            handle_change "$wt_path" "$branch" "$summary"
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
}

# ── Main loop ───────────────────────────────────────────────────────

main() {
    log "=== Refinement loop starting ==="
    log "Focus: $FOCUS_PATH"
    log "Types: ${ENABLED_TYPES[*]}"
    log "Idle limit: $IDLE_LIMIT"

    # Check if focus area is graduated
    if [[ -f "$LEDGER" ]]; then
        local area_status
        area_status="$(jq -r --arg key "$FOCUS_PATH" '.areas[$key].status // "null"' "$LEDGER" 2>/dev/null || echo "null")"
        if [[ "$area_status" == "graduated" ]]; then
            log "Focus area $FOCUS_PATH is graduated. Set status to 'active' in refine-ledger.json to re-run."
            exit 0
        fi
    fi

    local idle_count=0
    local pr_count=0
    local fail_count=0
    local max_failures=5

    while true; do
        local action
        action="$(run_iteration)" || {
            fail_count=$((fail_count + 1))
            log "Iteration failed ($fail_count / $max_failures), continuing after cooldown"
            if [[ "$fail_count" -ge "$max_failures" ]]; then
                log "ERROR: Reached $max_failures consecutive failures, stopping"
                break
            fi
            sleep "$((COOLDOWN > 5 ? COOLDOWN : 5))"
            continue
        }
        fail_count=0

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

        if [[ "$COOLDOWN" -gt 0 ]]; then
            log "Cooling down for ${COOLDOWN}s"
            sleep "$COOLDOWN"
        fi
    done

    log "=== Refinement loop finished ==="
    log "Summary: $pr_count PRs opened, $idle_count consecutive idle results"
}

main

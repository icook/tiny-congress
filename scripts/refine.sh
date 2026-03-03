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
    perl -0777 -p -e '
        s/\Q{{FOCUS_PATH}}\E/$ENV{FOCUS_PATH}/g;
        s/\Q{{GUIDANCE_CONTENT}}\E/$ENV{GUIDANCE_CONTENT}/g;
        s/\Q{{ENABLED_TYPES}}\E/$ENV{ENABLED_TYPES}/g;
        s/\Q{{PENDING_CHANGES}}\E/$ENV{PENDING_CHANGES}/g;
    ' "$template_file"
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
    log "=== DRY RUN: Generated prompt ==="
    build_prompt "$(get_pending_pr_summary)"
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
    git -C "$wt_path" push --quiet origin "$branch"
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

    # Build prompt (includes pending PR info so Claude avoids duplicates)
    local prompt
    prompt="$(build_prompt "$pending_summary")"

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

    log "Result: action=$action summary=$summary"

    case "$action" in
        change)
            handle_change "$wt_path" "$branch" "$summary"
            ;;
        ticket)
            handle_ticket "$json_block" "$summary"
            cleanup_worktree "$wt_path" "$branch"
            ;;
        clean)
            log "Focus area clean: $summary"
            cleanup_worktree "$wt_path" "$branch"
            ;;
        *)
            log "WARNING: Unknown action '$action', treating as error"
            cleanup_worktree "$wt_path" "$branch"
            return 1
            ;;
    esac

    echo "$action"
}

# ── Main loop ───────────────────────────────────────────────────────

main() {
    log "=== Refinement loop starting ==="
    log "Focus: $FOCUS_PATH"
    log "Types: ${ENABLED_TYPES[*]}"
    log "Idle limit: $IDLE_LIMIT"

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
            clean)
                idle_count=$((idle_count + 1))
                log "Idle count: $idle_count / $IDLE_LIMIT"
                if [[ "$idle_count" -ge "$IDLE_LIMIT" ]]; then
                    log "Reached idle_limit=$IDLE_LIMIT, focus area clean"
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

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
FOCUS_GLOB="$(read_config '.focus.glob // ""')"
GUIDANCE_FILE="$(require_config '.prompts.guidance')"
COOLDOWN="$(require_config '.behavior.cooldown')"
MAX_PRS="$(require_config '.behavior.max_prs')"
IDLE_LIMIT="$(require_config '.behavior.idle_limit')"

# Parse enabled types into a list (priority order: security > patterns > tests > cleanup)
ENABLED_TYPES=()
[[ "$(read_config '.types.security_hardening')" == "true" ]] && ENABLED_TYPES+=("security_hardening")
[[ "$(read_config '.types.pattern_enforcement')" == "true" ]] && ENABLED_TYPES+=("pattern_enforcement")
[[ "$(read_config '.types.test_coverage')" == "true" ]] && ENABLED_TYPES+=("test_coverage")
[[ "$(read_config '.types.code_cleanup')" == "true" ]] && ENABLED_TYPES+=("code_cleanup")

log() {
    local msg="[$(date '+%Y-%m-%d %H:%M:%S')] $*"
    echo "$msg"
    mkdir -p "$LOG_DIR"
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
    local template_file="$SCRIPT_DIR/refine-prompt.md"
    if [[ ! -f "$template_file" ]]; then
        log "ERROR: Prompt template not found at $template_file"
        exit 1
    fi
    local template
    template="$(cat "$template_file")"

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

    # Substitute placeholders
    template="${template//\{\{FOCUS_PATH\}\}/$FOCUS_PATH}"
    template="${template//\{\{GUIDANCE_CONTENT\}\}/$guidance}"
    template="${template//\{\{ENABLED_TYPES\}\}/$types_section}"

    echo "$template"
}

if $DRY_RUN; then
    log "=== DRY RUN: Generated prompt ==="
    build_prompt
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
        --label "refinement,auto-merge")"
    log "Created PR: $pr_url"

    # Enable auto-merge — don't fail the iteration if this fails
    gh pr merge "$pr_url" --auto --squash || \
        log "WARNING: Could not enable auto-merge for $pr_url (may need manual merge)"
    log "Auto-merge enabled for $pr_url"
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
    existing="$(gh issue list --label "refinement,needs-design" \
        --search "in:title $title" --json number --jq '.[0].number' 2>/dev/null || echo "")"

    if [[ -n "$existing" ]]; then
        log "Duplicate finding, existing issue #$existing"
        return 0
    fi

    local issue_url
    issue_url="$(gh issue create \
        --title "$title" \
        --body "$body" \
        --label "refinement,needs-design")"
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

    # Build prompt
    local prompt
    prompt="$(build_prompt)"

    # Run Claude Code in the worktree
    local claude_output
    local claude_exit=0
    claude_output="$(
        cd "$wt_path" && \
        claude -p \
            --output-format json \
            --tools "Read Edit Write Bash Glob Grep" \
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
            sed -n '/^```json/,/^```/{/^```/d;p}' | \
            jq -c '.' 2>/dev/null || echo "")"
        # Verify it has an action field
        if [[ -n "$json_block" ]] && ! echo "$json_block" | jq -e '.action' &>/dev/null; then
            json_block=""
        fi
    fi

    # Strategy 3: scan text for a JSON object containing "action" using python
    if [[ -z "$json_block" ]]; then
        json_block="$(python3 -c "
import json, re, sys
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

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

FOCUS_PATH="$(read_config '.focus.path')"
FOCUS_GLOB="$(read_config '.focus.glob // ""')"
GUIDANCE_FILE="$(read_config '.prompts.guidance')"
COOLDOWN="$(read_config '.behavior.cooldown')"
MAX_PRS="$(read_config '.behavior.max_prs')"
IDLE_LIMIT="$(read_config '.behavior.idle_limit')"

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
    local template
    template="$(cat "$SCRIPT_DIR/refine-prompt.md")"

    # Read guidance content
    local guidance=""
    if [[ -f "$REPO_ROOT/$GUIDANCE_FILE" ]]; then
        guidance="$(cat "$REPO_ROOT/$GUIDANCE_FILE")"
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

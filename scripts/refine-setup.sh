#!/usr/bin/env bash
set -euo pipefail

# Provision GitHub secrets required by the refinement workflow.
#
# Required secrets:
#   CLAUDE_CODE_OAUTH_TOKEN — Long-lived OAuth token from Claude Max/Pro (for `claude -p`)
#   REFINE_PAT              — Fine-grained GitHub PAT so pushed branches trigger CI
#
# Usage: ./scripts/refine-setup.sh

REPO="$(gh repo view --json nameWithOwner -q .nameWithOwner 2>/dev/null)" || {
    echo "ERROR: Not in a GitHub repo or gh CLI not authenticated."
    exit 1
}

OWNER="${REPO%%/*}"
REPO_NAME="${REPO##*/}"

echo "=== Refinement Workflow Setup ==="
echo "Repository: $REPO"
echo ""

# ── Helper ────────────────────────────────────────────────────────────────────

check_secret() {
    gh secret list --json name -q ".[].name" 2>/dev/null | grep -qx "$1"
}

# ── CLAUDE_CODE_OAUTH_TOKEN ──────────────────────────────────────────────────

echo "1. Checking CLAUDE_CODE_OAUTH_TOKEN..."

if check_secret "CLAUDE_CODE_OAUTH_TOKEN"; then
    echo "   Already set."
else
    echo ""
    echo "   CLAUDE_CODE_OAUTH_TOKEN is not set."
    echo "   This is a long-lived OAuth token for 'claude -p' in CI."
    echo ""
    echo "   Generating token via 'claude setup-token'..."
    echo "   (This requires a Claude Max/Pro subscription.)"
    echo ""

    if ! command -v claude &>/dev/null; then
        echo "   ERROR: claude CLI not found. Install it first:"
        echo "     npm install -g @anthropic-ai/claude-code"
        echo ""
        echo "   Then re-run this script."
    else
        claude setup-token
        echo ""
        echo "   Paste the generated token below."
        echo ""
        read -rsp "   CLAUDE_CODE_OAUTH_TOKEN (hidden): " oauth_token
        echo ""

        if [[ -z "$oauth_token" ]]; then
            echo "   Skipped — set it later with: gh secret set CLAUDE_CODE_OAUTH_TOKEN"
        else
            echo "$oauth_token" | gh secret set CLAUDE_CODE_OAUTH_TOKEN
            echo "   Set CLAUDE_CODE_OAUTH_TOKEN."
        fi
    fi
fi

echo ""

# ── REFINE_PAT ────────────────────────────────────────────────────────────────

echo "2. Checking REFINE_PAT..."

if check_secret "REFINE_PAT"; then
    echo "   Already set."
else
    echo ""
    echo "   REFINE_PAT is not set."
    echo ""
    echo "   The default GITHUB_TOKEN cannot trigger CI on pushed branches."
    echo "   A fine-grained PAT scoped to this repo is needed so that PRs"
    echo "   created by the refinement loop get their CI checks run."
    echo ""
    echo "   Create one at:"
    echo "     https://github.com/settings/personal-access-tokens/new"
    echo ""
    echo "   Settings:"
    echo "     Token name:       refine-$REPO_NAME"
    echo "     Expiration:       90 days"
    echo "     Resource owner:   $OWNER"
    echo "     Repository access: Only select repositories → $REPO_NAME"
    echo ""
    echo "   Permissions (Repository):"
    echo "     Actions:        Read (to check CI status)"
    echo "     Contents:       Read & Write (to push branches)"
    echo "     Issues:         Read & Write (to add labels)"
    echo "     Pull requests:  Read & Write (to create PRs)"
    echo "     Metadata:       Read (always included)"
    echo ""
    echo "   After creating, paste the token below."
    echo ""
    read -rsp "   REFINE_PAT (hidden): " pat
    echo ""

    if [[ -z "$pat" ]]; then
        echo "   Skipped — set it later with: gh secret set REFINE_PAT"
    else
        echo "$pat" | gh secret set REFINE_PAT
        echo "   Set REFINE_PAT."
    fi
fi

echo ""

# ── Verify ────────────────────────────────────────────────────────────────────

echo "=== Verification ==="

missing=()
check_secret "CLAUDE_CODE_OAUTH_TOKEN" || missing+=("CLAUDE_CODE_OAUTH_TOKEN")
check_secret "REFINE_PAT"              || missing+=("REFINE_PAT")

if [[ ${#missing[@]} -eq 0 ]]; then
    echo "All secrets configured. You can now run:"
    echo "  just refine-remote          # trigger manually"
    echo "  gh workflow run refine.yml  # same thing"
    echo ""
    echo "The workflow also runs on schedule (every 4 hours)."
else
    echo "Missing secrets: ${missing[*]}"
    echo "Set them with: gh secret set <NAME>"
fi

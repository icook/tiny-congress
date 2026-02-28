# Autonomous Code Quality Pipeline — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the informational-only PR review with a single-pass fix+flag workflow, and add an adversarial testing workflow that autonomously writes tests to break the architecture.

**Architecture:** Two GitHub Actions workflows powered by `claude-code-action@v1`. PR polish runs on every push with scoped write access (fix what's fixable, flag what's not). Adversarial testing runs on-demand/weekly with full tool access, writes tests on a branch, and opens a draft PR with findings.

**Tech Stack:** GitHub Actions, `anthropics/claude-code-action@v1`, existing Rust test infrastructure (`#[shared_runtime_test]`, testcontainers, `TestAppBuilder`)

**Design doc:** `docs/plans/2026-02-28-autonomous-code-quality-design.md`

---

## Task 1: Write the PR polish prompt file

**Files:**
- Create: `.github/prompts/pr-polish.md`

**Step 1: Create the prompt file**

```markdown
# PR Auto-Polish

You are an automated code quality assistant for the TinyCongress project. Your job is to fix what you can and flag what you can't.

## Context

- This is a Rust + React monorepo. Backend in `service/`, frontend in `web/`, shared crypto in `crates/tc-crypto/`.
- The project uses Ed25519 cryptographic identity. The trust boundary rule: crypto operations happen in the browser only. The server never handles plaintext private keys.
- Coding standards: `docs/interfaces/rust-coding-standards.md`. Styling: Mantine-first (ADR-005). Naming: snake_case Rust, PascalCase React components, camelCase hooks.

## Phase 1: Fix

Read the PR diff with `gh pr diff`. Fix anything that is **unambiguously wrong**:

- Formatting violations (rustfmt, prettier)
- Lint issues (clippy warnings, eslint errors)
- Naming convention mismatches (e.g. camelCase in Rust, snake_case in React)
- Missing error handling that follows an established pattern already present in the same file or module
- Unused imports or variables
- Typos in comments or strings

Run `just fmt` to auto-format, then review the diff for remaining issues and apply targeted fixes with the Edit tool.

After all fixes, create a **single** commit:
```
git add -A
git commit -m "chore: auto-polish [auto-polish]"
git push
```

**DO NOT fix:**
- Logic changes, even if you think they're improvements
- Anything touching `crates/tc-crypto/` public API
- Anything that changes a public API signature (endpoint, GraphQL field, response shape)
- Code outside the PR diff
- Anything you're not 100% confident about

If there is nothing to fix, skip this phase entirely. Do not create an empty commit.

## Phase 2: Flag

For issues that require human judgment, leave **inline comments** using `mcp__github_inline_comment__create_inline_comment`. Only flag:

- Potential security issues or trust boundary violations
- Logic that looks intentional but may have edge-case bugs
- Design questions where the approach is unclear
- Missing test coverage for non-trivial logic

Do NOT flag:
- Style preferences beyond the established conventions
- Suggestions to "improve" code that works correctly
- Anything you already fixed in Phase 1

## Phase 3: Summary

Post ONE PR comment via `gh pr comment` with this structure:

```
## Auto-Polish Summary

**Fixed:** [count] issues in [commit SHA or "no fixes needed"]
- [one-line description of each fix category]

**Flagged:** [count] items for human review
- [one-line description of each flag, referencing inline comments]

**Skipped:** Nothing outside the PR diff was touched.
```

## Hard Rules

- NEVER add features, refactor adjacent code, or make "improvements"
- NEVER touch files outside the PR diff
- NEVER modify CI/CD configuration, GitHub workflows, or CLAUDE.md
- NEVER run tests (you don't have the tools for it)
- One commit maximum. If nothing needs fixing, zero commits.
```

**Step 2: Verify the file exists and reads correctly**

Run: `cat .github/prompts/pr-polish.md | head -5`
Expected: The file header appears.

**Step 3: Commit**

```bash
git add .github/prompts/pr-polish.md
git commit -m "feat: add PR auto-polish prompt file"
```

---

## Task 2: Write the adversarial testing prompt file

**Files:**
- Create: `.github/prompts/adversarial.md`

**Step 1: Create the prompt file**

```markdown
# Adversarial Architecture Testing

You are a security-focused test engineer for the TinyCongress project. Your job is to write tests that try to break the system, then report what you find.

## Context

- TinyCongress uses Ed25519 cryptographic identity. Users generate key pairs client-side. The server never sees private key material.
- Core abstractions: Account (username + root public key), Device Key (delegated Ed25519 key certified by root key), Backup Envelope (password-encrypted root private key, Argon2id KDF), KID (key identifier = base64url(SHA-256(pubkey)[0:16]), exactly 22 chars).
- Trust boundary: crypto operations happen in the browser via `tc-crypto` WASM. The backend validates signatures and envelope structure only.
- See `docs/domain-model.md` for full entity schemas and invariant tables.
- See `CLAUDE.md` for trust boundary rules and design principles.

## Test Infrastructure

- Test files go in `service/tests/` with suffix `_tests.rs`
- Use `#[shared_runtime_test]` macro from `tc_test_macros` (NOT `#[tokio::test]`)
- Use `common::test_db::isolated_db()` for database-backed tests
- Use `common::app_builder::TestAppBuilder` to construct test apps
- Use `common::factories::valid_signup_with_keys()` for signup with key access
- Use `tower::ServiceExt::oneshot()` to send requests
- Model new tests after `service/tests/identity_handler_tests.rs`
- DO NOT add new dependencies to `Cargo.toml`

## Focus Area: Trust Boundary Probing

Write tests that attempt to:
- Submit signup requests with the `root_pubkey` field containing a private key (should be rejected or the field should only be used as a public key)
- Submit requests that attempt to bypass signature verification on device key certificates
- Craft requests that would cause the server to return decrypted backup envelope material
- Submit forged device key certificates (signed by wrong key, self-signed, truncated signatures)
- Attempt to use a revoked device key for authenticated operations

## Focus Area: API Robustness

Write tests that attempt to:
- Send malformed JSON bodies to signup/auth endpoints
- Send oversized payloads (>1MB bodies)
- Submit KIDs with wrong length (21, 23, 0, 256 chars), invalid base64url characters
- Submit empty strings, null bytes, unicode edge cases for username
- Submit backup envelopes with KDF params below OWASP minimums (m_cost < 19456, t_cost < 2)
- Race condition: concurrent signup with same username
- Race condition: concurrent device key add/revoke

## Focus Area: Domain Logic Edge Cases

Write tests that attempt to:
- Add an 11th device key to an account (max is 10)
- Use a device key certificate signed by a different account's root key
- Submit a backup envelope with mismatched version bytes
- Register with a root public key that produces a KID collision with an existing account
- Authenticate with a device key whose certificate has been tampered with (bit-flip)

## Output Protocol

1. Create a new test file: `service/tests/adversarial_tests.rs`
2. Add `mod common;` at the top (required for shared test utilities)
3. Write each test with a clear doc comment explaining the attack vector
4. Run the tests: `cargo test --test adversarial_tests -- --test-threads=1`
5. Record results: which tests pass (system correctly defends), which fail (potential bug)

After writing and running tests:

1. Create a branch: `git checkout -b adversarial/$(date +%Y-%m-%d)-{focus}`
2. Commit the test file: `git add service/tests/adversarial_tests.rs && git commit -m "test: adversarial testing — {focus} focus"`
3. Push and open a draft PR:
```bash
git push origin adversarial/$(date +%Y-%m-%d)-{focus}
gh pr create --draft --title "test: adversarial testing — {focus}" --body "$(cat <<'EOF'
## Adversarial Testing Results

**Focus:** {focus}
**Date:** $(date +%Y-%m-%d)

### Findings

[For each test, report:]
- **Test name**: description of attack vector
- **Result**: PASS (system defended correctly) / FAIL (potential bug)
- **Severity** (if FAIL): Critical / High / Medium / Low

### Summary

- Tests written: N
- Defenses confirmed: N
- Potential bugs found: N

### Next Steps

[List any bugs that need issues filed]
EOF
)"
```

4. For any FAIL results with severity High or Critical, create a GitHub issue:
```bash
gh issue create --title "Security: [brief description]" --label "bug,security" --body "[details]"
```

## Hard Rules

- ONLY write test files. NEVER modify production code.
- ONLY use existing test infrastructure. NEVER add dependencies.
- If a test requires infrastructure that doesn't exist, skip it and note why.
- Document every test's intent clearly — another developer should understand the attack vector from the doc comment.
```

**Step 2: Verify the file exists**

Run: `cat .github/prompts/adversarial.md | head -5`
Expected: The file header appears.

**Step 3: Commit**

```bash
git add .github/prompts/adversarial.md
git commit -m "feat: add adversarial testing prompt file"
```

---

## Task 3: Create the PR polish workflow

**Files:**
- Create: `.github/workflows/claude-pr-polish.yml`

**Step 1: Write the workflow file**

```yaml
name: Claude PR Polish

on:
  pull_request:
    types: [opened, synchronize, ready_for_review, reopened]

concurrency:
  group: polish-${{ github.event.pull_request.number }}
  cancel-in-progress: true

jobs:
  polish:
    # Skip auto-polish commits to prevent infinite loops
    if: "!contains(github.event.pull_request.head.sha && github.event.head_commit.message || '', '[auto-polish]')"
    runs-on: ubuntu-latest
    permissions:
      contents: write
      pull-requests: write
      issues: read
      id-token: write

    steps:
      - name: Check for auto-polish commit
        id: check
        run: |
          COMMIT_MSG=$(gh api repos/${{ github.repository }}/commits/${{ github.event.pull_request.head.sha }} --jq .commit.message)
          if echo "$COMMIT_MSG" | grep -q '\[auto-polish\]'; then
            echo "skip=true" >> "$GITHUB_OUTPUT"
          else
            echo "skip=false" >> "$GITHUB_OUTPUT"
          fi
        env:
          GH_TOKEN: ${{ github.token }}

      - name: Checkout repository
        if: steps.check.outputs.skip != 'true'
        uses: actions/checkout@v4
        with:
          ref: ${{ github.event.pull_request.head.ref }}
          fetch-depth: 0

      - name: Read prompt file
        if: steps.check.outputs.skip != 'true'
        id: prompt
        run: |
          {
            echo "PROMPT<<PROMPT_EOF"
            echo "REPO: ${{ github.repository }}"
            echo "PR NUMBER: ${{ github.event.pull_request.number }}"
            echo "PR BRANCH: ${{ github.event.pull_request.head.ref }}"
            echo ""
            cat .github/prompts/pr-polish.md
            echo ""
            echo "PROMPT_EOF"
          } >> "$GITHUB_OUTPUT"

      - name: Run Claude PR Polish
        if: steps.check.outputs.skip != 'true'
        uses: anthropics/claude-code-action@v1
        with:
          claude_code_oauth_token: ${{ secrets.CLAUDE_CODE_OAUTH_TOKEN }}
          prompt: ${{ steps.prompt.outputs.PROMPT }}
          claude_args: |
            --allowedTools "Read,Glob,Grep,Edit,Write,Bash(git add:*),Bash(git commit:*),Bash(git push:*),Bash(just fmt:*),Bash(just lint:*),Bash(gh pr diff:*),Bash(gh pr comment:*),Bash(gh pr view:*),mcp__github_inline_comment__create_inline_comment"
```

**Step 2: Validate YAML syntax**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/claude-pr-polish.yml'))"`
Expected: No error output (valid YAML).

**Step 3: Commit**

```bash
git add .github/workflows/claude-pr-polish.yml
git commit -m "feat: add PR auto-polish workflow"
```

---

## Task 4: Create the adversarial testing workflow

**Files:**
- Create: `.github/workflows/claude-adversarial.yml`

**Step 1: Write the workflow file**

```yaml
name: Claude Adversarial Testing

on:
  workflow_dispatch:
    inputs:
      focus:
        description: "Testing focus area"
        required: true
        type: choice
        options:
          - trust-boundary
          - api-robustness
          - domain-logic
          - all
  schedule:
    # Weekly on Sunday at 03:00 UTC, rotating focus via prompt
    - cron: "0 3 * * 0"

concurrency:
  group: adversarial
  cancel-in-progress: true

jobs:
  adversarial:
    runs-on: ubuntu-latest
    permissions:
      contents: write
      pull-requests: write
      issues: write
      id-token: write

    steps:
      - name: Checkout repository
        uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Determine focus area
        id: focus
        run: |
          if [ "${{ github.event_name }}" = "workflow_dispatch" ]; then
            echo "area=${{ github.event.inputs.focus }}" >> "$GITHUB_OUTPUT"
          else
            # Rotate weekly: week number mod 3
            WEEK=$(date +%U)
            case $((WEEK % 3)) in
              0) echo "area=trust-boundary" >> "$GITHUB_OUTPUT" ;;
              1) echo "area=api-robustness" >> "$GITHUB_OUTPUT" ;;
              2) echo "area=domain-logic" >> "$GITHUB_OUTPUT" ;;
            esac
          fi

      - name: Read prompt file
        id: prompt
        run: |
          {
            echo "PROMPT<<PROMPT_EOF"
            echo "REPO: ${{ github.repository }}"
            echo "FOCUS AREA: ${{ steps.focus.outputs.area }}"
            echo "DATE: $(date +%Y-%m-%d)"
            echo ""
            cat .github/prompts/adversarial.md
            echo ""
            echo "Your focus for this run is: ${{ steps.focus.outputs.area }}"
            echo "If the focus is 'all', cover all three areas. Otherwise, focus exclusively on the specified area."
            echo "PROMPT_EOF"
          } >> "$GITHUB_OUTPUT"

      - name: Set up Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Run Claude Adversarial Testing
        uses: anthropics/claude-code-action@v1
        with:
          claude_code_oauth_token: ${{ secrets.CLAUDE_CODE_OAUTH_TOKEN }}
          prompt: ${{ steps.prompt.outputs.PROMPT }}
          claude_args: |
            --allowedTools "Read,Glob,Grep,Edit,Write,Bash(cargo:*),Bash(just:*),Bash(git:*),Bash(gh:*),Bash(date:*),Bash(cat:*)"
```

**Step 2: Validate YAML syntax**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/claude-adversarial.yml'))"`
Expected: No error output (valid YAML).

**Step 3: Commit**

```bash
git add .github/workflows/claude-adversarial.yml
git commit -m "feat: add adversarial testing workflow"
```

---

## Task 5: Delete the old review workflow

**Files:**
- Delete: `.github/workflows/claude-code-review.yml`

**Step 1: Remove the file**

```bash
git rm .github/workflows/claude-code-review.yml
```

**Step 2: Commit**

```bash
git commit -m "chore: remove old code review workflow (replaced by pr-polish)"
```

---

## Task 6: Verify the full setup

**Step 1: Check all new files exist**

Run: `ls -la .github/workflows/claude-pr-polish.yml .github/workflows/claude-adversarial.yml .github/prompts/pr-polish.md .github/prompts/adversarial.md`
Expected: All four files listed.

**Step 2: Check old file is gone**

Run: `ls .github/workflows/claude-code-review.yml`
Expected: `No such file or directory`

**Step 3: Validate both workflow files parse as YAML**

Run: `python3 -c "import yaml; [yaml.safe_load(open(f)) for f in ['.github/workflows/claude-pr-polish.yml', '.github/workflows/claude-adversarial.yml']]"`
Expected: No error.

**Step 4: Check git log shows clean history**

Run: `git log --oneline -5`
Expected: 4-5 focused commits matching the tasks above.

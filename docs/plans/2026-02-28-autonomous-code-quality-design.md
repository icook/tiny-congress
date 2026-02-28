# Autonomous Code Quality Pipeline

**Date:** 2026-02-28
**Status:** Proposed

## Problem

Three related pain points in the current workflow:

1. **Manual fix loop.** Claude reviews PRs and leaves comments. Applying obvious fixes (formatting, naming, lint) by hand is busywork.
2. **Stale reviews.** The review takes long enough that the code moves on. Comments reference lines or logic that have already changed.
3. **Untested architecture.** The crypto trust boundary, API surface, and domain logic haven't been adversarially tested. The prototype needs to be pushed until it breaks.

## Design

One unified system with two workflows: a PR polish pass that fixes and flags, and an adversarial testing pass that tries to break the architecture.

### 1. PR Auto-Fix + Review (`claude-pr-polish.yml`)

Replaces `claude-code-review.yml`. Single `claude-code-action` invocation with write access.

**Triggers:** `opened`, `synchronize`, `ready_for_review`, `reopened`.

**Permissions:**
- `contents: write` (push fixup commits)
- `pull-requests: write` (post comments, inline comments)

**Tool access (scoped):**
- Read/search: `Read`, `Glob`, `Grep`
- Edit: `Edit`, `Write`
- Git: `Bash(git add:*)`, `Bash(git commit:*)`, `Bash(git push:*)`
- Lint/format: `Bash(just lint:*)`, `Bash(just fmt:*)`
- Review: `mcp__github_inline_comment__create_inline_comment`, `Bash(gh pr comment:*)`, `Bash(gh pr diff:*)`, `Bash(gh pr view:*)`

**Three phases, executed in order:**

1. **Fix.** Read the diff. Fix anything unambiguously wrong: lint violations, formatting, naming convention mismatches, missing error handling that follows established patterns in the codebase. Push a single fixup commit with message `chore: auto-polish [auto-polish]`.

2. **Flag.** For anything requiring human judgment — design questions, ambiguous intent, potential security or trust-boundary issues — leave an inline comment. These should be rare after the fix phase.

3. **Summary.** Post one PR comment summarizing what was fixed (with commit SHA) and what was flagged.

**Anti-runaway guards:**
- Skip runs where HEAD commit message contains `[auto-polish]` (prevents re-trigger loop)
- Concurrency group `polish-${{ github.event.pull_request.number }}` with `cancel-in-progress: true`
- Scoped tool access — no `Bash(*)` wildcard, no test execution, no CI modification
- Prompt explicitly forbids: adding features, refactoring adjacent code, changing public API signatures, touching files outside the diff

**Why this fixes staleness:** Fixes happen in the same invocation that identifies them. No gap between "identify issue" and "apply fix." If a new push arrives during the run, the concurrency group cancels the stale run and starts fresh.

### 2. Adversarial Testing (`claude-adversarial.yml`)

New workflow. Runs independently from the PR flow.

**Triggers:**
- `workflow_dispatch` with input to select focus area (`trust-boundary`, `api-robustness`, `domain-logic`, `all`)
- `schedule`: weekly cron, rotating focus
- Optionally on `push` to `master` when `service/src/**`, `crates/tc-crypto/**`, or `migrations/**` change

**Permissions:**
- `contents: write` (push test branch, open PR)
- `pull-requests: write` (create draft PR)
- `issues: write` (label findings)

**Tool access:** Full — `Bash(cargo:*)`, `Bash(just:*)`, `Read`, `Glob`, `Grep`, `Edit`, `Write`.

**Three focus areas:**

1. **Trust boundary probing.** Craft API requests that attempt to: trick the server into handling private key material, bypass signature verification, return decrypted backup envelopes, accept forged device key certificates. Write as integration tests in `service/tests/`.

2. **API robustness.** Malformed GraphQL queries, oversized payloads, missing required fields, invalid KIDs (wrong length, bad characters), duplicate registrations, race conditions on device key operations. Write as `*_tests.rs` following existing patterns.

3. **Domain logic edge cases.** Boundary conditions: max 10 device keys + 1, backup envelope with KDF params below OWASP minimums, expired/revoked device key used for auth, KID collisions. Write tests that assert correct rejection.

**Output:** Claude pushes a branch (`adversarial/YYYY-MM-DD-<focus>`), opens a draft PR summarizing what it tried, what passed, what broke. Findings that expose bugs get labeled `bug`/`security` via `gh`.

**Scope constraints:**
- Test files only — no production code modifications
- Must use existing test infrastructure (`#[shared_runtime_test]`, testcontainers)
- No new dependencies without flagging in PR description
- Concurrency group `adversarial` — one run at a time

### 3. Prompt Files

Prompts stored as committed, reviewable files rather than inline YAML:

- `.github/prompts/pr-polish.md` — fix/flag/summary instructions, scope boundaries, trust boundary rules
- `.github/prompts/adversarial.md` — focus areas, test conventions, what constitutes a finding vs a confirmed defense

Workflows load these via `prompt_file:` or by reading them at runtime.

### 4. What Stays

`claude.yml` (the `@claude` mention handler) is unchanged. It serves a different purpose: interactive, on-demand assistance.

## Files Changed

| Action | File |
|--------|------|
| Delete | `.github/workflows/claude-code-review.yml` |
| Create | `.github/workflows/claude-pr-polish.yml` |
| Create | `.github/workflows/claude-adversarial.yml` |
| Create | `.github/prompts/pr-polish.md` |
| Create | `.github/prompts/adversarial.md` |

## Rollout

1. **PR polish first.** Swap out `claude-code-review.yml`. Test on a few PRs. Tune the prompt based on results.
2. **Adversarial second.** Add the workflow. Run manually (`workflow_dispatch`) a few times to calibrate. Enable the weekly cron once the output is consistently useful.

## Risks

- **Write access on PRs.** Claude can now push commits. Mitigated by scoped tool access, anti-runaway guards, and the `[auto-polish]` skip pattern. Branch protection rules still apply — Claude can't push to `master`.
- **Adversarial tests may be noisy.** Early runs will likely produce tests that fail for wrong reasons (test setup issues, not real bugs). Calibration period is expected.
- **Prompt drift.** As the codebase evolves, prompts may become stale. Treat prompt files like code — review changes, update when conventions shift.

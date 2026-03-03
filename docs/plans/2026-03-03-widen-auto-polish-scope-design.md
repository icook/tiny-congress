# Widen Auto-Polish Fix Scope

**Date:** 2026-03-03
**Status:** Approved

## Problem

The auto-polish workflow (`claude-pr-polish.yml` + `.github/prompts/pr-polish.md`) flags many low-risk issues as inline review comments that could be auto-fixed without human judgment. On PR #368, all four flagged issues (accessibility attrs, testid placement, stale relative time, redundant tooltip) were mechanical fixes that required a follow-up commit. This adds friction to the PR cycle.

## Decision

Widen the Phase 1 "What to Fix" section to cover five new categories of auto-fixable issues. Keep the same single-commit workflow, same hard constraints, and same Phase 2 flag list.

## New Auto-Fix Categories

### 1. Accessibility on interactive elements

When a PR adds a clickable element (`onClick`) that lacks keyboard accessibility:
- Add `role="button"` and `tabIndex={0}`
- Add `onKeyDown` handler for Enter/Space that calls the same handler
- When `data-testid` is on a non-interactive child but the click handler is on a parent, move the testid to the interactive element

Only apply to elements introduced in the PR diff.

### 2. Unused variables and dead bindings in new code

- Remove destructured variables that are never read (e.g., `const { isLoading, data } = useQuery(...)` where `isLoading` is unused)
- In Rust, prefix unused parameters with `_`
- In TypeScript, remove unused destructured bindings entirely
- Already covers unused imports; this extends to unused bindings

### 3. Comment/code mismatches

When a comment describes behavior that demonstrably contradicts the adjacent code (e.g., "both MIN and MAX overflow" when only MIN does), rewrite the comment to match the code. Do not change code to match comments — comments are always the less-trustworthy source.

### 4. Doc precision

Fix factual inaccuracies in doc comments: wrong units (chars vs bytes for `str::len()`), wrong types, wrong return values. Only when the correct answer is unambiguous from the function signature or body.

### 5. Consistent build recipe dependencies

When a justfile recipe operates on frontend assets but is missing the `_ensure-frontend-deps` dependency that sibling recipes use, add it. Same pattern for any other standard dependency that every recipe of a given type uses.

## What Stays Flag-Only (Phase 2)

No changes. These remain flag-only:
- Security concerns (trust boundary, timing, injection)
- Architectural design questions
- Test coverage gaps
- Type safety upgrades (String -> newtype)
- Trust boundary violations
- Performance improvements

## New Guard Rail

Add to "What NOT to Fix":
- Accessibility patterns that would change component API or require new dependencies

## Scope

- Only file changed: `.github/prompts/pr-polish.md`
- No workflow YAML changes needed
- No new CI permissions or tools required

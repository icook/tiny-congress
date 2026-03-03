# Widen Auto-Polish Fix Scope — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Expand the auto-polish prompt so it auto-fixes low-risk issues (accessibility, unused bindings, comment accuracy, doc precision, recipe consistency) instead of only flagging them.

**Architecture:** Single prompt file edit — add new bullet points to Phase 1 "What to Fix", add a guard rail to "What NOT to Fix", no workflow or tooling changes.

**Tech Stack:** Markdown prompt (`.github/prompts/pr-polish.md`)

---

## Task 1: Add new fix categories to Phase 1

**Files:**
- Modify: `.github/prompts/pr-polish.md:36-42`

**Step 1: Add the five new categories after the existing "Lint violations" bullet (line 42)**

Append these bullets after the existing list in "### What to Fix":

```markdown
- **Accessibility on interactive elements:** When a PR adds a clickable element (`onClick`) without keyboard accessibility, add `role="button"`, `tabIndex={0}`, and an `onKeyDown` handler for Enter/Space that calls the same handler. When `data-testid` is on a non-interactive child but the click handler is on a parent wrapper, move the testid to the interactive element. Only fix elements introduced in the PR diff.
- **Unused variables and dead bindings:** Remove destructured variables that are never read (e.g., `const { isLoading, data } = useQuery(...)` where `isLoading` is unused — drop it from the destructuring). In Rust, prefix unused parameters with `_`. In TypeScript, remove the unused binding entirely. This extends the existing "unused imports" rule to unused local bindings.
- **Comment/code mismatches:** When a comment describes behavior that demonstrably contradicts the adjacent code (e.g., "both MIN and MAX overflow" when only MIN does), rewrite the comment to match the code. Never change code to match a comment — comments are always the less-trustworthy source.
- **Doc precision:** Fix factual inaccuracies in doc comments — wrong units (chars vs bytes for `str::len()`), wrong types, wrong return values — but only when the correct answer is unambiguous from the function signature or body.
- **Consistent build recipe dependencies:** When a justfile recipe operates on frontend assets but is missing the `_ensure-frontend-deps` dependency that sibling recipes use, add it. Apply the same pattern for any standard dependency that every recipe of a given type uses.
```

**Step 2: Verify the edit reads correctly**

Read back `.github/prompts/pr-polish.md` lines 36-55 and confirm the new bullets flow naturally after the existing ones.

**Step 3: Commit**

```bash
git add .github/prompts/pr-polish.md
git commit -m "chore(polish): add five new auto-fix categories to Phase 1"
```

---

## Task 2: Add guard rail to "What NOT to Fix"

**Files:**
- Modify: `.github/prompts/pr-polish.md:56-63` (the "What NOT to Fix" section)

**Step 1: Add new exclusion bullet**

After the existing "Anything that changes observable behavior" bullet, add:

```markdown
- Accessibility patterns that would change a component's public API (props interface) or require adding new dependencies
```

**Step 2: Verify the edit**

Read back the "What NOT to Fix" section and confirm it's consistent.

**Step 3: Commit**

```bash
git add .github/prompts/pr-polish.md
git commit -m "chore(polish): add accessibility guard rail to What NOT to Fix"
```

---

## Task 3: Run lint and verify prompt structure

**Step 1: Verify no formatting issues in the markdown**

Read the full file and confirm:
- No broken markdown tables or lists
- Section numbering is consistent
- No duplicate bullets between "What to Fix" and "What to Flag"

**Step 2: Commit design doc**

```bash
git add docs/plans/2026-03-03-widen-auto-polish-scope-design.md docs/plans/2026-03-03-widen-auto-polish-scope.md
git commit -m "docs: add design doc for widened auto-polish scope"
```

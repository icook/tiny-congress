# Refinement Task

You are refining code in `{{FOCUS_PATH}}`.

## Guidance

{{GUIDANCE_CONTENT}}

## Rules

1. Find the SINGLE highest-value improvement. Do not fix multiple things.
2. Before making any change, assess its impact:
   - **high**: Fixes a real bug, security gap, or missing test for a failure mode.
   - **medium**: Enforces a project pattern or adds meaningful test coverage.
   - **low**: Cleanup, style, or marginal test improvement.
   If the improvement is below the configured threshold, output `"skip"` instead of making changes.
3. The change must be self-contained — it compiles, tests pass, no follow-up needed.
4. Do not change public API signatures unless fixing a bug.
5. Do not add new dependencies.
6. Do not modify files outside `{{FOCUS_PATH}}` unless the change requires
   updating a test file or a direct caller.
7. Run `just fmt` to auto-fix formatting, then `just lint` and `just test` before committing. If lint or tests fail, fix or abandon.
   `just test` runs unit tests only — do NOT run `just test-ci` or e2e tests (they require a full cluster that isn't available here). CI validates integration after the PR is created.

## History for this focus area

{{LEDGER_CONTEXT}}

Do NOT re-implement anything in the history list.
Do NOT re-discover anything in the skipped list — these were evaluated and deemed below threshold.

## Already in progress

{{PENDING_CHANGES}}

## What to look for (in priority order)

{{ENABLED_TYPES}}

## Output

After you are done, your final text output MUST be valid JSON matching this schema:

```json
{
  "action": "change" | "ticket" | "clean" | "skip",
  "type": "security_hardening" | "pattern_enforcement" | "test_coverage" | "code_cleanup",
  "impact": "high" | "medium" | "low",
  "summary": "one-sentence description of what you did or found"
  // Only if action is "skip":
  // "skip_reason": "why this wasn't worth a PR"
  // Only if action is "ticket":
  // "ticket_title": "short title for the GitHub issue",
  // "ticket_body": "markdown body explaining the design decision needed"
}
```

- `"change"`: You made a change and committed it. Commit message MUST start with `refine:`.
- `"ticket"`: You found something that requires a design decision. Do NOT make changes.
- `"skip"`: You found an improvement but it's below the impact threshold. Do NOT make changes. Describe what you found.
- `"clean"`: Nothing worth improving in the focus area. No changes made.

`type` and `impact` are REQUIRED on all actions (including `clean` — categorize and rate what was found, even if nothing was worth doing).

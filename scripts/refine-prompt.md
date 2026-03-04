# Refinement Task

You are refining code in `{{FOCUS_PATH}}`.

## Guidance

{{GUIDANCE_CONTENT}}

## Rules

1. Find the SINGLE highest-value improvement. Do not fix multiple things.
2. The change must be self-contained — it compiles, tests pass, no follow-up needed.
3. Do not change public API signatures unless fixing a bug.
4. Do not add new dependencies.
5. Do not modify files outside `{{FOCUS_PATH}}` unless the change requires
   updating a test file or a direct caller.
6. Run `just fmt` to auto-fix formatting, then `just lint` and `just test` before committing. If lint or tests fail, fix or abandon.
   `just test` runs unit tests only — do NOT run `just test-ci` or e2e tests (they require a full cluster that isn't available here). CI validates integration after the PR is created.

## Already in progress

{{PENDING_CHANGES}}

## What to look for (in priority order)

{{ENABLED_TYPES}}

## Output

After you are done, your final text output MUST be valid JSON matching this schema:

```json
{
  "action": "change" | "ticket" | "clean",
  "summary": "one-sentence description of what you did or found"
  // Only if action is "ticket":
  // "ticket_title": "short title for the GitHub issue",
  // "ticket_body": "markdown body explaining the design decision needed",
  // "ticket_labels": ["refinement", "needs-design"]
}
```

- `"change"`: You made a change and committed it. Commit message MUST start with `refine:`.
- `"ticket"`: You found something that requires a design decision. Do NOT make changes. Describe the decision needed.
- `"clean"`: Nothing worth improving in the focus area. No changes made.

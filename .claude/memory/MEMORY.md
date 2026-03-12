# Project Memory

## Workflow

- **Subagents don't reliably run lint.** Always verify lint in the main session before committing or after subagent commits.
- `CLAUDE.md` is a symlink to `AGENTS.md` — edits land in `AGENTS.md`.

## Plan Quality

- **Plans must include type signatures for new types and key functions.** A plan that says "create account with root_kid" without specifying the type will produce stringly-typed code. Review function signatures in the plan before approving.
- **During brainstorming, challenge primitive types.** If a value has format rules, ask "should this be a newtype?" before moving to implementation planning.

## ESLint Conventions (web/)

- `prefer-nullish-coalescing` is a warning but `--max-warnings=0` makes it a hard fail
- Features (`src/features/`) cannot use deep relative imports (`../*/**`) — use `@/` path alias instead
- `public/` directory is ignored by ESLint
- Barrel import enforcement: import from sibling barrel (`../api`), not internals (`../api/client`)

## CI / Workflows

- Auto-polish workflow: `.github/workflows/claude-pr-polish.yml` + `.github/prompts/pr-polish.md`
- Scope (PR #377): auto-fixes accessibility attrs, unused TS bindings, comment/code mismatches, doc precision, recipe dep consistency

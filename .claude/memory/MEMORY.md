# Project Memory

## Workflow Rules

- **Always run `just lint-frontend` before committing frontend changes.** The pre-commit hook (`lint-staged`) only catches staged TS/TSX files — it won't flag ESLint config issues, new file patterns, or missing ignores. Running the full lint suite avoids surprises.
- **Run `yarn install` in `web/` if `node_modules` seems stale.** Missing packages (like `eslint-plugin-unicorn`) cause lint to fail with confusing module-not-found errors.
- **Subagents don't reliably run lint.** Always verify lint in the main session before committing or after subagent commits.

## File Conventions

- Ephemeral designs/specs go in `.plan/` (not committed to PRs)
- Throwaway working notes go in `.scratch/` (deleted after task)
- Do NOT create `docs/plans/` — only permanent docs belong in `docs/`
- `CLAUDE.md` is a symlink or alias for `AGENTS.md` — edits land in `AGENTS.md`

## Plan Quality

- **Plans must include type signatures for new types and key functions.** A plan that says "create account with root_kid" without specifying the type will produce stringly-typed code. Review function signatures in the plan before approving.
- **During brainstorming, challenge primitive types.** If a value has format rules, ask "should this be a newtype?" before moving to implementation planning.

## Architectural Decisions

- **Never recommend a workaround to avoid a structural fix.** When a task is hard because the underlying abstraction is wrong, say so and propose fixing the abstraction. "Keeps X out of scope" is not a justification for adding complexity — especially pre-launch where structural changes carry no migration risk.

## Security Review Handling

- **Never defer security review comments without a ticket.** When the automated reviewer or a human flags a security concern, either fix it in the PR or create a GitHub issue with a clear description before marking the comment resolved.
- **"Not exploitable today" is not the bar.** The bar is: would a security-focused reviewer see this and conclude the authors were careful?

## ESLint Conventions (web/)

- `curly` rule enforced — always use braces with `if` statements
- `prefer-nullish-coalescing` is a warning but `--max-warnings=0` makes it a hard fail
- Features (`src/features/`) cannot use deep relative imports (`../*/**`) — use `@/` path alias instead
- `public/` directory is ignored by ESLint (added in runtime-env work)
- Barrel import enforcement: import from sibling barrel (`../api`), not internals (`../api/client`)

## CI / Workflows

- Auto-polish workflow: `.github/workflows/claude-pr-polish.yml` + `.github/prompts/pr-polish.md`
- Auto-polish scope (PR #377): auto-fixes accessibility attrs, unused TS bindings, comment/code mismatches, doc precision, recipe dep consistency
- **E2E Test Optimization** (March 2026): Replaced KinD/Skaffold with direct process execution in CI integration-tests job. Docker-based PostgreSQL testcontainer, binary execution with env vars. Estimated 40-50% latency reduction (25-35 min → 15-20 min).

## Completed Work (reference)

- **Ticket 381: Generalized Verifier API** (merged PR #384) — verifiers are accounts (not a separate entity type), device key auth (no API keys), genesis verifiers from `TC_VERIFIERS` env var, `tc-api-client` crate via Progenitor. Design doc: `docs/plans/2026-03-03-generalized-verifier-api-design.md`.
- **Demo readiness checklist** at `docs/demo-readiness-checklist.md` — living doc for Mar 20 friends-and-family demo.

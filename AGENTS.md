# Repository Guidelines

This file is a concise index of rules and pointers—not detailed documentation. Target: ~100-150 lines. Move verbose explanations to `docs/` or directory READMEs.

## Default Delivery Flow
- Always sync with `master` before starting work: `git checkout master && git pull --rebase`. When picking up a PR or ticket, rebase your working branch onto the refreshed `master` before any edits.
- Create a branch for every ticket before making changes following the conventions in `docs/interfaces/branch-naming-conventions.md` (e.g., `feature/123-update-copy`, `fix/456-login-redirect`).
- Implement the work, keep commits focused, and run the relevant test suites via `just test` (unit tests) or `just test-ci` (full CI suite).
- Treat each major checkpoint on a PR (e.g., before opening, after rebasing, after addressing review) as a moment to leave a fresh status comment with what changed and what still needs attention.
- Once everything passes locally, push the branch and open a draft PR that links the tracked issue, fills out the PR template, and notes which AI tool generated it (if applicable).
- After the PR's automated checks succeed, mark it ready for review.
- Respond to review feedback when critiques are obviously sensible improvements.
- Stick to this loop unless the issue description calls out an alternative rollout path.
- When continuing a rebase in the CLI harness, run `GIT_EDITOR=true git rebase --continue`; otherwise Git attempts to launch `vim`, which hangs the workflow.

## Project Structure & Module Organization
- `service/`: Rust GraphQL API, workers, and SQL migrations (`migrations/`). Tests live in `service/tests/` (`*_tests.rs`).
- `web/`: React/Mantine client on Vite. Source under `web/src/`, shared mocks in `web/test-utils/`.
- `dockerfiles/`, `skaffold.yaml`, `kube/`: Container and Kubernetes assets for CI, integration, and demo environments.

## Working Documents

| Location | Purpose | Lifecycle |
|----------|---------|-----------|
| `.scratch/` | Ephemeral notes, analysis, brainstorming | Delete after task |
| `.plan/` | Feature specs, tickets for large branches | Remove on merge to master |
| `docs/` | Accepted documentation only | Permanent |

See README in each directory for details. Never put scratch/analysis files in `docs/`.

## Documentation (Required Reading)
Consult these before starting work:

| Directory | Purpose | When to read |
|-----------|---------|--------------|
| `docs/playbooks/` | Step-by-step how-to guides | Before any unfamiliar task |
| `docs/interfaces/` | Contracts, schemas, naming rules | Before writing new code |
| `docs/decisions/` | ADRs explaining why decisions were made | When questioning existing patterns |
| `docs/checklists/` | Pre-PR, pre-release, incident checklists | Before opening PRs or deploying |
| `docs/style/` | UI styling guidelines (Mantine-first) | Before modifying frontend UI |
| `docs/skills/` | LLM skills for specific tasks | When referenced by other docs |

Key playbooks:
- `docs/playbooks/local-dev-setup.md` - Development environment options
- `docs/playbooks/adding-migration.md` - Database changes
- `docs/playbooks/new-graphql-endpoint.md` - API changes
- `docs/playbooks/debugging-ci-failure.md` - CI troubleshooting

Key interfaces:
- `docs/interfaces/environment-variables.md` - Required and optional env vars
- `docs/interfaces/directory-conventions.md` - Where code lives
- `docs/interfaces/naming-conventions.md` - How to name things
- `docs/interfaces/branch-naming-conventions.md` - Branch naming standards
- `docs/interfaces/agent-output-schema.md` - PR compliance format

Key style guides:
- `docs/style/STYLE_GUIDE.md` - Mantine-first styling policy
- `docs/style/LLM_UI_GUIDE.md` - LLM instructions for UI work

## Build, Test, and Development Commands

Use the `justfile` as the single source of truth for all commands. Run `just --list` to see all available recipes.

| Task | Command | Notes |
|------|---------|-------|
| **Linting** | `just lint` | Runs backend + frontend linting |
| **Formatting** | `just fmt` | Fixes all formatting issues |
| **Unit tests** | `just test` | Backend + frontend unit tests (no cluster) |
| **Full CI suite** | `just test-ci` | Builds images, runs all tests via Skaffold |
| **Backend only** | `just lint-backend`, `just test-backend` | |
| **Frontend only** | `just lint-frontend`, `just test-frontend` | |
| **Type checking** | `just typecheck` | Frontend TypeScript checking |
| **Dev server** | `just dev` | Full-stack with Skaffold (requires cluster) |
| **Frontend dev** | `just dev-frontend` | Vite dev server only |
| **Build** | `just build` | Backend + frontend builds |

Additional notes:
- CI monitoring: after pushing a branch, run `gh run watch --branch $(git rev-parse --abbrev-ref HEAD)` to stream workflow progress.
- Rust Docker builds use cargo-chef stages by default; keep the planner/cacher/builder structure intact when editing `service/Dockerfile*` assets.
- **Local Kubernetes:** `just dev` requires KinD (`kind create cluster`). Use KinD for CI parity. See `docs/playbooks/local-dev-setup.md`.

## Coding Style & Naming Conventions
- Rust uses edition 2021 with rustfmt; keep modules snake_case and favor descriptive crate names.
- Prefer `Result<_, anyhow::Error>` for async handlers so GraphQL resolvers surface uniform errors.
- Frontend TypeScript relies on Prettier, ESLint, Stylelint; use PascalCase components, camelCase hooks, and co-locate styles.
- Frontend styling follows Mantine-first approach per ADR-005 (`docs/decisions/005-mantine-first-styling.md`).

## Testing Guidelines
- Keep specs near code (`*_tests.rs`, `*.test.tsx`). Reuse fixtures before adding mocks.
- Cover ranking, pairing, and voting flows when rules shift; add regression tests for reported bugs.
- Run `just lint` and `just test` for quick local validation; run `just test-ci` for full CI suite before PRs.
- Treat the `testing local dev` LLM skill (`docs/skills/testing-local-dev.md`) as a pre-merge requirement for any MR that changes Skaffold configuration; document the results in the PR.

## Commit & Pull Request Guidelines
- Match the concise, imperative commit log (e.g., `Migrate CI build to docker build-push`). Avoid bundling unrelated work.
- PR descriptions should cover intent, risks, rollout, and linked issues. Add screenshots or GraphQL traces for UX or schema changes.
- Call out env var updates and refresh `docs/` entries when system behavior evolves.

## Environment & Configuration Tips
- Keep secrets out of version control; export `DATABASE_URL` and queue settings locally and in CI.
- Ensure PostgreSQL loads `CREATE EXTENSION pgmq;` before integration jobs.
- Align Docker tags with `skaffold.yaml` profiles so preview, test, and prod images stay consistent.

## Prohibited Actions (Hard Constraints)
- DO NOT modify files outside `service/`, `web/`, `kube/`, `dockerfiles/`, `docs/`, or repo root configs
- DO NOT add new database tables or migrations without explicit approval
- DO NOT add dependencies without updating the appropriate lockfile (`Cargo.lock`, `yarn.lock`)
- DO NOT push directly to `master`; all changes go through PRs
- DO NOT skip CI checks or use `--no-verify` flags
- DO NOT commit secrets, credentials, or `.env` files
- DO NOT delete or rename existing public API endpoints without deprecation
- DO NOT modify `skaffold.yaml` profiles without running the testing-local-dev skill
- DO NOT run bare `git push`; ALWAYS specify the remote and branch explicitly: `git push origin <branch-name>` (see ADR-004)
- DO NOT use `--force` or `--force-with-lease` without explicit branch: `git push --force-with-lease origin <branch-name>`

## Agent Acknowledgement Contract
Every agent-generated PR description MUST include this YAML block at the end:

```yaml
# --- Agent Compliance ---
agent_compliance:
  docs_read:
    - AGENTS.md
  constraints_followed: true
  files_modified: []  # List paths actually changed
  deviations:
    - none  # Or explain any rule exceptions
```

CI will reject PRs missing this block or with malformed YAML. The `files_modified` list must match the actual diff.

## Recovery Protocol
If an agent violates these rules:
1. Revert the offending commit: `git revert <sha>`
2. Open an issue documenting the violation
3. Update AGENTS.md if the rule was unclear

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

## Documentation

See [docs/README.md](docs/README.md) for the full index of playbooks, interfaces, ADRs, checklists, and document placement rules.

Key references:
- `docs/playbooks/` - How-to guides (local-dev-setup, adding-migration, new-graphql-endpoint)
- `docs/interfaces/` - Contracts and naming conventions
- `docs/interfaces/ticket-management.md` - Labeling taxonomy for GitHub issues (follow when creating tickets)
- `docs/decisions/` - ADRs explaining architectural choices
- `docs/style/` - UI styling (Mantine-first per ADR-005)

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
| **Security audit** | `just audit` | All security checks (blocking in CI) |
| **Dep vulnerabilities** | `just audit-deps` | cargo-deny + yarn audit |
| **Secret detection** | `just audit-secrets` | gitleaks scan |
| **Unused deps** | `just audit-unused` | cargo-machete |

Additional notes:
- CI monitoring: after pushing a branch, run `gh run watch --branch $(git rev-parse --abbrev-ref HEAD)` to stream workflow progress.
- Rust Docker builds use cargo-chef stages by default; keep the planner/cacher/builder structure intact when editing `service/Dockerfile*` assets.
- **Local Kubernetes:** `just dev` requires KinD (`kind create cluster`). Use KinD for CI parity. See `docs/playbooks/local-dev-setup.md`.

## Coding Style & Naming Conventions
- Rust uses edition 2021 with rustfmt; keep modules snake_case and favor descriptive crate names.
- Prefer `Result<_, anyhow::Error>` for async handlers so GraphQL resolvers surface uniform errors.
- Frontend TypeScript relies on Prettier, ESLint, Stylelint; use PascalCase components, camelCase hooks, and co-locate styles.
- Frontend styling follows Mantine-first approach per ADR-005 (`docs/decisions/005-mantine-first-styling.md`).

## Design Principles

TinyCongress handles cryptographic identity and delegation. The bar is: code that is obviously correct, not merely code that appears to work. These principles serve that standard.

- **Make wrong code hard to write.** Prefer types and APIs where misuse is a compile error, not a runtime bug. A function that accepts `&Kid` instead of `&str` turns a class of bugs into type errors. A `BackupEnvelope` that can only be constructed through parsing turns malformed data into an early, obvious failure. This matters doubly for AI-assisted development: LLMs optimize for "compiles and passes tests", not "makes incorrect usage structurally impossible." Explicit type-level constraints counteract that tendency. See `docs/interfaces/rust-coding-standards.md` for patterns.
- **Strict by default, paranoid at boundaries.** Reject input that is technically parseable but outside expected parameters — don't rely on "this should be safe" or "no reasonable client would send this." If Argon2id m_cost should be >= 65536 in practice, enforce that. If a field should be exactly 22 characters, reject 23. Prefer breaking on unexpected input over silently accepting it: a crash from a violated assumption is better than undefined behavior from an assumption that turned out to be wrong. This applies to configuration too — use `${VAR:?error message}` over `${VAR:-default}`.
- **No untracked security debt.** Every security-relevant concern raised in review must be either fixed in the PR or tracked as a GitHub issue before merge. "We'll fix it later" without a ticket is how things get forgotten. A knowledgeable reviewer should see deliberate choices, not deferred sloppiness — even in a preview.
- **Single source of truth:** Configuration values (versions, ports, feature flags) should be defined in exactly one place. Other files should read from or reference that source, not duplicate the value.
- **Don't ship dead code paths:** If only one variant exists (one KDF algorithm, one envelope version), don't add dispatch logic or database columns for hypothetical future variants. Add them when the second variant arrives. Unused branches are untested branches.

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
- Backend tests use testcontainers for DB isolation. Run `just build-test-postgres` once to build the custom image.
- Align Docker tags with `skaffold.yaml` profiles so preview, test, and prod images stay consistent.
- Follow the secure defaults policy (`docs/interfaces/secure-defaults.md`) when adding configuration options.

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

## Recovery Protocol
If an agent violates these rules:
1. Revert the offending commit: `git revert <sha>`
2. Open an issue documenting the violation
3. Update AGENTS.md if the rule was unclear

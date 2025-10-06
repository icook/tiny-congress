# Repository Guidelines

## Default Delivery Flow
- Always sync with `master` before starting work: `git checkout master && git pull --rebase`. When picking up a PR or ticket, rebase your working branch onto the refreshed `master` before any edits.
- Create a feature branch for every ticket before making changes; include the GitHub issue number and a short, descriptive slug (e.g., `feature/123-update-copy`).
- Implement the work, keep commits focused, and run the relevant backend (`cargo test`) and frontend (`yarn test`) suites.
- Treat each major checkpoint on a PR (e.g., before opening, after rebasing, after addressing review) as a moment to leave a fresh status comment with what changed and what still needs attention.
- Once everything passes locally, push the branch and open a draft PR that links the tracked issue, fills out the Codex PR template, and explicitly notes `Opened by: Codex`.
- After the PR's automated checks succeed, mark it ready for review.
- Wait for the Copilot review to land and respond to critiques when they are obviously sensible improvements.
- Stick to this loop unless the issue description calls out an alternative rollout path.

## Project Structure & Module Organization
- `service/`: Rust GraphQL API, workers, and SQL migrations (`migrations/`). Tests live in `service/tests/` (`*_tests.rs`).
- `web/`: React/Mantine client on Vite. Source under `web/src/`, shared mocks in `web/test-utils/`.
- `doc/` and `adr/`: Architecture notes and decision records—refresh them when contracts change. Treat `doc/tickets/` entries as temporary scaffolding; once a ticket is opened, delete the scratch file so the issue tracker stays the single source of truth.
- `dockerfiles/`, `docker-compose.test.yml`, `skaffold.yaml`, `kube/`: Container and Kubernetes assets for CI, integration, and demo environments.

## Build, Test, and Development Commands
- Backend loop: `cd service && cargo check`, `cargo fmt`, `cargo clippy --all-targets -- -D warnings` keep builds clean.
- Backend tests: `cargo test` for unit/API coverage; `./run_integration_tests.sh` or `docker-compose -f ../docker-compose.test.yml up --build` runs PostgreSQL-backed suites.
- Frontend workflows: `cd web && yarn install` once, then `yarn dev` (Vite server), `yarn build` (production assets), `yarn preview` (smoke test).
- Frontend quality gates: `yarn lint`, `yarn typecheck`, `yarn prettier`, `yarn vitest`; CI `yarn test` chains them.
- Full-stack verification: prefer `skaffold test -p ci` and `skaffold verify -p ci` (add `--build-artifacts <file>` when reusing prebuilt images) to mirror CI behavior; `skaffold dev -p dev` remains available for interactive loops.
- CI monitoring: after pushing a branch, run `gh run watch --branch $(git rev-parse --abbrev-ref HEAD)` to stream workflow progress.
- Rust Docker builds use cargo-chef stages by default; keep the planner/cacher/builder structure intact when editing `service/Dockerfile*` assets.

## Coding Style & Naming Conventions
- Rust uses edition 2021 with rustfmt; keep modules snake_case and favor descriptive crate names.
- Prefer `Result<_, anyhow::Error>` for async handlers so GraphQL resolvers surface uniform errors.
- Frontend TypeScript relies on Prettier, ESLint, Stylelint; use PascalCase components, camelCase hooks, and co-locate styles.

## Testing Guidelines
- Keep specs near code (`*_tests.rs`, `*.test.tsx`). Reuse fixtures before adding mocks.
- Cover ranking, pairing, and voting flows when rules shift; add regression tests for reported bugs.
- Run `skaffold test -p ci`, and `skaffold verify -p ci` (optionally reusing `--build-artifacts <file>`) before PRs

## Commit & Pull Request Guidelines
- Match the concise, imperative commit log (e.g., `Migrate CI build to docker build-push`). Avoid bundling unrelated work.
- PR descriptions should cover intent, risks, rollout, and linked issues. Add screenshots or GraphQL traces for UX or schema changes.
- Call out env var updates and refresh `doc/` or `adr/` entries when system behavior evolves.

## Environment & Configuration Tips
- Keep secrets out of version control; export `DATABASE_URL` and queue settings locally and in CI.
- Ensure PostgreSQL loads `CREATE EXTENSION pgmq;` before integration jobs.
- Align Docker tags with `skaffold.yaml` profiles so preview, test, and prod images stay consistent.

# Repository Guidelines

This file is a concise index of rules and pointers—not detailed documentation. Keep it scannable; every section should earn its place. Move detailed reference material to `docs/` or directory READMEs.

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

## Domain Model

TinyCongress is a community governance platform built around cryptographic identity. Users generate Ed25519 key pairs client-side; the server never sees private key material. The trust model is: **the server is a dumb witness, not a trusted authority.**

**Core abstractions:**

- **Account** — identified by username + root public key. The root key is the highest-privilege credential (meant for cold storage). All authority traces back to it.
- **Device Key** — a delegated Ed25519 key for daily use. Root key signs a certificate over the device key to prove authorization. Max 10 per account. Revocable but not rotatable (revoke and re-delegate instead).
- **Backup Envelope** — password-encrypted root private key stored on server. Binary format with Argon2id KDF (OWASP 2024 minimums enforced). Server stores ciphertext; decryption happens client-side only.
- **KID (Key Identifier)** — `base64url(SHA-256(pubkey)[0:16])`, always exactly 22 characters. Deterministic, stable reference to any public key.

**Trust boundary:** Crypto operations (key generation, signing, envelope encryption/decryption) happen in the browser via `tc-crypto` WASM. The backend validates signatures and envelope structure but never handles plaintext key material. Code that blurs this boundary is a security bug. The server is still responsible for platform-level availability and compliance (rate limiting, abuse detection, GDPR deletion) — orthogonal to the crypto trust model.

For detailed entity schemas, binary formats, validation rules, and invariant tables, see [`docs/domain-model.md`](docs/domain-model.md).

## Trust Boundary Rules
- DO NOT write server code that accepts, logs, stores, or processes plaintext private keys
- DO NOT add server-side signing, encryption, or key generation — these happen in the browser only
- DO NOT log request bodies on auth/signup endpoints (contain key material in transit)
- DO NOT add endpoints that return decrypted backup material
- Changes to `crates/tc-crypto/` affect both backend (native) and frontend (WASM) — test both sides

## Decision Authority
**Proceed without asking:** Bug fixes with clear reproduction, test additions, lint/formatting fixes, doc updates that match current code.

**Ask before proceeding:** New dependencies, API surface changes (new endpoints, changed response shapes), anything touching crypto/auth/the trust boundary, changes to shared types used by both GraphQL and REST, changes to `tc-crypto` public API.

**Never without explicit approval:** Database migrations, changing KDF parameters or envelope format, deleting or renaming public API endpoints.

## Common Mistakes
- Using `String` where a newtype exists (`Kid`, `BackupEnvelope`). If a domain type exists, use it — the type system is the guardrail.
- Adding `match` arms, database columns, or dispatch logic for variants that don't exist yet (second KDF algorithm, second envelope version). Wait until the second variant arrives.
- Assuming error mappings without checking the service layer. The HTTP status for a repo error depends on context (e.g., `DuplicateAccount` on backup is 500 during signup because the account was just created in the same transaction).
- Confabulating API names. Verify method signatures against actual code before documenting or calling them (e.g., `Kid::from_str()` exists; `Kid::parse()` does not).
- "Improving" code adjacent to the task — don't refactor, add docstrings, or clean up surrounding code unless asked.
- Adding safety theater: `unwrap_or_default()`, redundant `Option` wrapping, or defensive clones that hide bugs instead of surfacing them. If something shouldn't be None, let it fail visibly.
- Inventing new patterns instead of matching existing ones. Before writing a new handler, repo method, or test, find the closest existing example and follow its structure.

## Documentation

See [docs/README.md](docs/README.md) for the full index of playbooks, interfaces, ADRs, checklists, and document placement rules.

Key references:
- `docs/domain-model.md` - Core entities, trust boundaries, and data invariants
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
| **Type checking** | `just lint-typecheck` | Frontend TypeScript checking (included in `just lint`) |
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
- **Simple over easy.** When choosing between a tactical shortcut and a structural improvement, prefer the structural improvement — especially early in the project when the cost of change is low. Assembling a database URL in a Helm template avoids touching Rust code (easy) but adds coupling and complexity. Refactoring the Rust config to accept individual fields is more work upfront (simple) but produces a better abstraction. LLMs default to minimizing scope — "option 1 keeps X out of scope" — which sounds disciplined but accumulates structural debt. The right question is not "what's the smallest change?" but "what leaves the system easier to change next time?"
- **One correct path, not two.** When the same operation exists in multiple code paths, invariants drift — one gets the lock, the other doesn't; one validates, the other assumes. Consolidate to a single implementation and have callers delegate to it. Two paths that do the same thing aren't redundancy, they're a bug that hasn't diverged yet.
- **Don't ship dead code paths:** If only one variant exists (one KDF algorithm, one envelope version), don't add dispatch logic or database columns for hypothetical future variants. Add them when the second variant arrives. Unused branches are untested branches.

## Verification Requirements
Before claiming work is complete:
- **Any code change**: `just lint` and `just test` pass
- **Rust type changes**: Check if the type is used in GraphQL (`SimpleObject`) AND REST (`ToSchema`) — update both derives
- **`tc-crypto` changes**: Run both `just test-backend` and `just test-frontend` — the crate compiles to native and WASM
- **Error handling changes**: Trace the error from repo → service (`map_signup_error`) → HTTP handler to confirm the correct status code reaches the client
- **New endpoints**: Add tests, update `docs/interfaces/api-contracts.md`, and run `just codegen` if types changed
- **Migration changes**: Verify `cargo sqlx prepare` succeeds and the `.sqlx/` query cache is updated

## Testing Guidelines
- Keep specs near code (`*_tests.rs`, `*.test.tsx`). Reuse fixtures before adding mocks.
- Cover ranking, pairing, and voting flows when rules shift; add regression tests for reported bugs.
- Run `just lint` and `just test` for quick local validation; run `just test-ci` for full CI suite before PRs.
- Treat the `testing local dev` LLM skill (`docs/skills/testing-local-dev.md`) as a pre-merge requirement for any MR that changes Skaffold configuration; document the results in the PR.

## High-Risk Areas
Extra scrutiny required for changes to:
- `crates/tc-crypto/` — shared crypto; changes affect both platforms silently
- `service/src/identity/service.rs` — signup validation and transaction orchestration
- `service/migrations/` — irreversible schema changes
- `kube/` and `skaffold.yaml` — production infrastructure
- Any file handling keys, signatures, or envelope parsing

## Commit & Pull Request Guidelines
- Match the concise, imperative commit log (e.g., `Migrate CI build to docker build-push`). Avoid bundling unrelated work.
- PR descriptions should cover intent, risks, rollout, and linked issues. Add screenshots or GraphQL traces for UX or schema changes.
- Call out env var updates and refresh `docs/` entries when system behavior evolves.

## Environment & Configuration Tips
- Keep secrets out of version control; export `DATABASE_URL` and queue settings locally and in CI.
- Backend tests use testcontainers for DB isolation. The custom postgres image is built automatically on first test run.
- Align Docker tags with `skaffold.yaml` profiles so preview, test, and prod images stay consistent.
- Follow the secure defaults policy (`docs/interfaces/secure-defaults.md`) when adding configuration options.

## Prohibited Actions (Hard Constraints)
- DO NOT modify files outside `service/`, `web/`, `kube/`, `dockerfiles/`, `docs/`, or repo root configs
- DO NOT add dependencies without updating the appropriate lockfile (`Cargo.lock`, `yarn.lock`)
- DO NOT push directly to `master`; all changes go through PRs
- DO NOT skip CI checks or use `--no-verify` flags
- DO NOT commit secrets, credentials, or `.env` files
- DO NOT modify `skaffold.yaml` profiles without running the testing-local-dev skill
- DO NOT run bare `git push`; ALWAYS specify the remote and branch explicitly: `git push origin <branch-name>` (see ADR-004)
- DO NOT use `--force` or `--force-with-lease` without explicit branch: `git push --force-with-lease origin <branch-name>`

## Recovery Protocol
If an agent violates these rules:
1. Revert the offending commit: `git revert <sha>`
2. Open an issue documenting the violation
3. Update AGENTS.md if the rule was unclear

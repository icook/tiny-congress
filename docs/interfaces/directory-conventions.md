# Directory Conventions

## Repository structure

```
tiny-congress/
├── AGENTS.md              # Agent instructions (read first)
├── README.md              # Project overview
├── skaffold.yaml          # Build/deploy orchestration
│
├── service/               # Rust backend
│   ├── src/               # Application code
│   ├── tests/             # Integration tests (*_tests.rs)
│   ├── migrations/        # SQL migrations (sqlx)
│   ├── bin/               # Helper scripts
│   ├── Cargo.toml         # Dependencies
│   ├── Cargo.lock         # Locked versions (commit this)
│   └── Dockerfile         # Production build
│
├── web/                   # React frontend
│   ├── src/               # Application code
│   │   ├── components/    # Shared React components
│   │   ├── hooks/         # Shared custom hooks
│   │   ├── pages/         # Shared route pages
│   │   ├── features/      # Feature modules (see below)
│   │   └── utils/         # Helpers
│   ├── tests/             # E2E tests (Playwright)
│   ├── test-utils/        # Shared test mocks
│   ├── package.json       # Dependencies
│   ├── yarn.lock          # Locked versions (commit this)
│   └── Dockerfile         # Production build
│
├── kube/                  # Kubernetes manifests
│   └── app/               # Helm chart
│       ├── Chart.yaml
│       ├── values.yaml
│       └── templates/     # K8s resource templates
│
├── dockerfiles/           # Shared Dockerfiles
│   └── Dockerfile.postgres  # Postgres + pgmq
│
├── docs/                  # Documentation
│   ├── playbooks/         # How-to guides
│   ├── interfaces/        # Contracts and schemas
│   ├── decisions/         # ADRs (why decisions were made)
│   ├── checklists/        # Pre-PR, release, incident
│   ├── style/             # UI styling guidelines
│   └── skills/            # LLM skills for specific tasks
│
└── .github/
    ├── workflows/         # CI/CD pipelines
    └── scripts/           # CI helper scripts
```

## Where to put new code

| Type | Location | Example |
|------|----------|---------|
| Rust module | `service/src/` | `service/src/voting.rs` |
| Rust test | `service/tests/` | `service/tests/voting_tests.rs` |
| SQL migration | `service/migrations/` | `service/migrations/20240101_add_votes.sql` |
| React component | `web/src/components/` | `web/src/components/VoteButton.tsx` |
| React page | `web/src/pages/` | `web/src/pages/Dashboard.tsx` |
| React hook | `web/src/hooks/` | `web/src/hooks/useVoting.ts` |
| Component test | Next to component | `web/src/components/VoteButton.test.tsx` |
| E2E test | `web/tests/` | `web/tests/voting.spec.ts` |
| K8s resource | `kube/app/templates/` | `kube/app/templates/cronjob.yaml` |
| Playbook | `docs/playbooks/` | `docs/playbooks/adding-cronjob.md` |
| ADR | `docs/decisions/` | `docs/decisions/003-use-pgmq.md` |

## Frontend feature modules

Use `web/src/features/{domain}/` for self-contained feature areas with their own API, state, and UI:

```
web/src/features/identity/
├── api/
│   └── client.ts       # API client + request/response types
├── keys/
│   ├── index.ts        # Barrel exports (public API)
│   ├── types.ts        # Type definitions
│   └── *.ts            # Implementation files
├── screens/
│   ├── Login.tsx       # Route pages for this feature
│   └── Login.test.tsx  # Co-located tests
├── state/
│   └── session.ts      # Feature state management
└── components/         # Feature-specific components (if needed)
```

**When to use features/**: Domain areas with dedicated API endpoints, state, and multiple screens. Examples: `identity`, `voting`, `moderation`.

**When to use top-level**: Shared components, hooks, or pages used across features.

## Rust domain modules

Large domains use a standard subdirectory pattern:

```
service/src/{domain}/
├── mod.rs          # Re-exports public API
├── crypto/         # Cryptographic operations
│   ├── mod.rs      # Re-exports + tests
│   └── *.rs        # Implementation files
├── http/           # HTTP handlers
│   ├── mod.rs      # Router composition
│   └── {resource}s.rs  # Endpoint handlers (plural: accounts, devices)
├── repo/           # Data persistence
│   └── *.rs        # Repository implementations
└── policy/         # Authorization (if needed)
```

**Standard subdirectories**:
- `crypto/` - Signing, verification, key derivation
- `http/` - Axum handlers and router
- `repo/` - Database queries and persistence
- `policy/` - Authorization rules
- `abuse/` - Rate limiting, audit logging

## Files that must stay in sync

| Primary | Dependent | Sync mechanism |
|---------|-----------|----------------|
| `Cargo.toml` | `Cargo.lock` | `cargo check` |
| `Cargo.toml` | `.sqlx/` | `cargo sqlx prepare` |
| `package.json` | `yarn.lock` | `yarn install` |
| `skaffold.yaml` | `.github/workflows/ci.yml` | Manual (image names) |

## Prohibited locations

- DO NOT add code outside defined directories
- DO NOT create new top-level directories without ADR
- DO NOT put tests in `src/` (use `tests/` or co-locate with `.test.` suffix)

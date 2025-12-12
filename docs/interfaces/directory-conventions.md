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
│   │   ├── components/    # React components
│   │   ├── hooks/         # Custom hooks
│   │   ├── pages/         # Route pages
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
├── doc/                   # Documentation
│   ├── playbooks/         # How-to guides
│   ├── interfaces/        # Contracts and schemas
│   ├── decisions/         # ADRs (why decisions were made)
│   └── checklists/        # Pre-PR, release, incident
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
| Playbook | `doc/playbooks/` | `doc/playbooks/adding-cronjob.md` |
| ADR | `doc/decisions/` | `doc/decisions/003-use-pgmq.md` |

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

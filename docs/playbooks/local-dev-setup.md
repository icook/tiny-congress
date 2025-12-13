# Local Development Setup

## When to use

- First-time setup on a new machine
- Running backend or frontend in isolation
- Development without Skaffold/Kubernetes
- Debugging with direct database access

## Prerequisites

Check installed tools:
```bash
just setup
```

Required:
- Rust (rustup)
- Node.js 22+ (see `web/.nvmrc`)
- PostgreSQL 15+ with pgmq extension
- just (`brew install just`)

## Quick Start Options

### Option A: Full Stack with Skaffold (Recommended)

Best for: Integration testing, production-like environment

```bash
kind create cluster      # Create local k8s cluster (use KinD for CI parity)
just dev                 # Builds images, deploys, hot-reloads
```

We use KinD (Kubernetes in Docker) for local development to match CI. KinD loads
images directly into the cluster without needing a registry.

Services available at:
- Frontend: http://localhost:5173
- GraphQL: http://localhost:8080/graphql

#### Hot Reload Behavior

With file sync enabled, code changes sync directly into running containers:
- **Frontend:** Vite HMR updates instantly (~100ms)
- **Backend:** cargo-watch recompiles inside container (~5-10s)

Changes to `Dockerfile*`, `Cargo.toml` dependencies, or `package.json` still trigger full rebuilds.

### Option B: Backend + Frontend Separately

Best for: Fast iteration, debugging one service

**Terminal 1 - Database:**
```bash
# Using Docker
docker run -d --name postgres \
  -e POSTGRES_PASSWORD=postgres \
  -e POSTGRES_DB=prioritization \
  -p 5432:5432 \
  ghcr.io/icook/tiny-congress/postgres:branch-master

# Or use local Postgres with pgmq extension
```

**Terminal 2 - Backend:**
```bash
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/prioritization
just dev-backend
```

**Terminal 3 - Frontend:**
```bash
just dev-frontend
```

### Option C: Frontend Only

Best for: UI work against staging/shared backend

```bash
just dev-frontend
```

Configure API endpoint in frontend if needed (currently hardcoded).

### Option D: Backend Only

Best for: API development, testing resolvers

```bash
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/prioritization
just dev-backend
```

GraphQL Playground: http://localhost:8080/graphql

## Database Setup

### Using Project Postgres Image (Recommended)

Includes pgmq extension pre-configured:

```bash
docker run -d --name tc-postgres \
  -e POSTGRES_PASSWORD=postgres \
  -e POSTGRES_DB=prioritization \
  -p 5432:5432 \
  ghcr.io/icook/tiny-congress/postgres:branch-master
```

### Using Local Postgres

1. Install pgmq extension (see https://github.com/tembo-io/pgmq)
2. Create database:
   ```bash
   createdb prioritization
   psql prioritization -c "CREATE EXTENSION pgmq;"
   ```

### Connecting to Existing Database

For debugging against staging/shared DB:

```bash
export DATABASE_URL=postgres://user:pass@staging-host:5432/prioritization
just dev-backend
```

## Running Tests

### Unit Tests (No Database)

```bash
just test-backend    # Rust tests
just test-frontend   # Vitest
just test            # Both
```

### Integration Tests (Requires Database)

```bash
# With local Postgres running:
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/prioritization
cd service && cargo test --test integration_tests
```

### E2E Tests (Requires Full Stack)

```bash
# Start backend + frontend first, then:
just test-frontend-e2e
```

## Common Workflows

### Making Backend Changes

```bash
# Terminal 1: Start backend with hot reload
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/prioritization
just dev-backend

# Terminal 2: Run tests as you work
just test-backend
just lint-backend
```

### Making Frontend Changes

```bash
# Start Vite dev server with HMR
just dev-frontend

# In another terminal, run tests
just test-frontend
just lint-frontend
```

### Database Schema Changes

```bash
# Create migration
cd service && sqlx migrate add my_migration

# Edit migrations/<timestamp>_my_migration.sql

# Run migration
DATABASE_URL=postgres://... sqlx migrate run

# Regenerate offline cache
cargo sqlx prepare
```

See `docs/playbooks/adding-migration.md` for full workflow.

## Port Reference

| Service | Default Port | Override |
|---------|--------------|----------|
| Frontend (Vite) | 5173 | `PORT=3000 just dev-frontend` |
| Backend (API) | 8080 | `PORT=9000 just dev-backend` |
| PostgreSQL | 5432 | Use different `DATABASE_URL` |
| GraphQL Playground | 8080 | Same as backend |

## Troubleshooting

### "connection refused" on backend start

Database not running or wrong host:
```bash
# Check if postgres is running
docker ps | grep postgres
# Or
pg_isready -h localhost -p 5432
```

### "relation does not exist" errors

Migrations not run:
```bash
cd service
DATABASE_URL=postgres://... sqlx migrate run
```

### Frontend can't reach backend

Check CORS and ports. Backend serves at 8080, frontend at 5173.

### "cargo watch" not found

Install cargo-watch:
```bash
cargo install cargo-watch
```

### Node version mismatch

```bash
just node-check    # See required version
cd web && nvm use  # Switch to correct version
```

### Slow backend compilation

Use release profile for faster runtime (slower initial build):
```bash
cargo run --release
```

Or use `mold` linker (Linux) / `zld` (macOS).

## IDE Setup

### VS Code

Recommended extensions:
- rust-analyzer (Rust)
- ESLint, Prettier (TypeScript)
- GraphQL (schema highlighting)

### Settings

```json
{
  "rust-analyzer.cargo.features": "all",
  "editor.formatOnSave": true
}
```

## See also

- `docs/interfaces/environment-variables.md` - All env vars
- `docs/playbooks/debugging-ci-failure.md` - When things break
- `docs/playbooks/adding-migration.md` - Database changes
- `README.md` - macOS initial setup

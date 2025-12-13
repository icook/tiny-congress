# Environment Variables

## Backend (Rust API)

### Required

| Variable | Description | Example |
|----------|-------------|---------|
| `DATABASE_URL` | PostgreSQL connection string | `postgres://postgres:postgres@localhost:5432/prioritization` |

### Optional

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | `8080` | HTTP server port |
| `RUST_LOG` | `info` | Log level (`debug`, `info`, `warn`, `error`) |
| `MIGRATIONS_DIR` | `<cargo_manifest>/migrations` | Custom migrations directory path |

### Test/CI Only

| Variable | Default | Description |
|----------|---------|-------------|
| `EXPORT_LCOV_BASE64` | `1` | Export coverage as base64 for CI artifacts |

## Frontend (React/Vite)

### Development

| Variable | Default | Description |
|----------|---------|-------------|
| `HOST` | `0.0.0.0` | Vite dev server host |
| `PORT` | `5173` | Vite dev server port |

### Testing (Playwright)

| Variable | Default | Description |
|----------|---------|-------------|
| `PLAYWRIGHT_BASE_URL` | `http://127.0.0.1:4173` | Base URL for E2E tests |
| `PLAYWRIGHT_COVERAGE` | - | Enable coverage collection when set |
| `CI` | - | Enables CI-specific behavior (retries, coverage) |

## Kubernetes Deployments

Environment variables are set in `kube/app/templates/deployment.yaml`:

```yaml
env:
  - name: RUST_LOG
    value: info
  - name: DATABASE_URL
    value: postgres://postgres:postgres@postgres:5432/prioritization
```

For production, use Kubernetes secrets:

```yaml
env:
  - name: DATABASE_URL
    valueFrom:
      secretKeyRef:
        name: postgres-credentials
        key: url
```

## Local Development

### Option 1: Export in shell

```bash
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/prioritization
export RUST_LOG=debug
just dev-backend
```

### Option 2: Use .env file (not committed)

Create `service/.env`:
```
DATABASE_URL=postgres://postgres:postgres@localhost:5432/prioritization
RUST_LOG=debug
```

The backend uses `dotenvy` to load `.env` files automatically.

### Option 3: Inline (one-off)

```bash
DATABASE_URL=postgres://... cargo run
```

## Connection String Format

PostgreSQL connection strings follow this format:

```
postgres://USER:PASSWORD@HOST:PORT/DATABASE
```

| Component | Local Default | CI Default |
|-----------|---------------|------------|
| USER | `postgres` | `postgres` |
| PASSWORD | `postgres` | `postgres` |
| HOST | `localhost` | `postgres` (k8s service) |
| PORT | `5432` | `5432` |
| DATABASE | `prioritization` | `prioritization` |

## Required Extensions

The database must have `pgmq` extension loaded:

```sql
CREATE EXTENSION IF NOT EXISTS pgmq;
```

This is handled automatically by `dockerfiles/Dockerfile.postgres`.

## Troubleshooting

| Error | Cause | Fix |
|-------|-------|-----|
| "connection refused" | Postgres not running | Start postgres or check host/port |
| "database does not exist" | DB not created | Run `createdb prioritization` |
| "extension pgmq does not exist" | Missing extension | Use provided Dockerfile.postgres |
| "RUST_LOG: invalid filter directive" | Bad log format | Use `debug`, `info`, `warn`, `error` |

## See also

- `service/src/main.rs` - Backend env var usage
- `kube/app/templates/deployment.yaml` - K8s configuration
- `dockerfiles/Dockerfile.postgres` - Database setup
- ADR-003: pgmq job queue

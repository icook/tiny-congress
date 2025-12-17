# Environment Variables

The backend uses [figment](https://docs.rs/figment/) for layered configuration. Configuration is loaded in priority order:

1. **Struct defaults** (lowest priority)
2. **config.yaml file** (if exists)
3. **Environment variables** with `TC_` prefix (highest priority, always wins)

## Backend (Rust API)

All environment variables use the `TC_` prefix. Nested config uses double underscore (`__`) separators.

### Database Configuration

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `TC_DATABASE__URL` | Yes | - | PostgreSQL connection string |
| `TC_DATABASE__MAX_CONNECTIONS` | No | `10` | Maximum connections in pool |
| `TC_DATABASE__MIGRATIONS_DIR` | No | auto-detect | Custom migrations directory path |

### Server Configuration

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `TC_SERVER__PORT` | No | `8080` | HTTP server port |
| `TC_SERVER__HOST` | No | `0.0.0.0` | HTTP server bind address |

### Logging Configuration

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `TC_LOGGING__LEVEL` | No | `info` | Log level (`debug`, `info`, `warn`, `error`) |

### CORS Configuration

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `TC_CORS__ALLOWED_ORIGINS` | No | `[]` (empty) | Comma-separated list of allowed origins |

**Security note:** CORS defaults to blocking all cross-origin requests. You must explicitly configure allowed origins.

Examples:
```bash
# Development
TC_CORS__ALLOWED_ORIGINS=http://localhost:5173,http://127.0.0.1:5173

# Production
TC_CORS__ALLOWED_ORIGINS=https://app.example.com

# Allow any origin (NOT recommended for production)
TC_CORS__ALLOWED_ORIGINS=*
```

### Build Info (unchanged)

| Variable | Default | Description |
|----------|---------|-------------|
| `APP_VERSION` | `dev` | Application version |
| `GIT_SHA` | `unknown` | Git commit SHA |
| `BUILD_TIME` | `unknown` | Build timestamp (RFC3339) |
| `BUILD_MESSAGE` | - | Optional build message |

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

## YAML Configuration

Create `service/config.yaml` for local development (see `service/config.yaml.example`):

```yaml
database:
  url: postgres://postgres:postgres@localhost:5432/tiny-congress
  max_connections: 10

server:
  port: 8080
  host: 0.0.0.0

logging:
  level: debug

# CORS: explicitly configure for development
cors:
  allowed_origins:
    - http://localhost:5173
    - http://127.0.0.1:5173
```

Environment variables always override YAML values.

## Kubernetes Deployments

Environment variables are set in `kube/app/templates/deployment.yaml`:

```yaml
env:
  - name: TC_LOGGING__LEVEL
    value: info
  - name: TC_DATABASE__URL
    value: postgres://postgres:postgres@postgres:5432/tiny-congress
  - name: TC_SERVER__PORT
    value: "8080"
  - name: TC_CORS__ALLOWED_ORIGINS
    value: "https://app.example.com"
```

For production, use Kubernetes secrets:

```yaml
env:
  - name: TC_DATABASE__URL
    valueFrom:
      secretKeyRef:
        name: postgres-credentials
        key: url
```

## Local Development

### Option 1: Environment variables

```bash
export TC_DATABASE__URL=postgres://postgres:postgres@localhost:5432/tiny-congress
export TC_LOGGING__LEVEL=debug
just dev-backend
```

### Option 2: YAML config file

```bash
cp service/config.yaml.example service/config.yaml
# Edit config.yaml as needed
just dev-backend
```

### Option 3: Inline (one-off)

```bash
TC_DATABASE__URL=postgres://... cargo run
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
| DATABASE | `tiny-congress` | `tiny-congress` |

## Required Extensions

The database must have `pgmq` extension loaded:

```sql
CREATE EXTENSION IF NOT EXISTS pgmq;
```

This is handled automatically by `dockerfiles/Dockerfile.postgres`.

## Troubleshooting

| Error | Cause | Fix |
|-------|-------|-----|
| "database.url is required" | Missing TC_DATABASE__URL | Set `TC_DATABASE__URL` environment variable |
| "connection refused" | Postgres not running | Start postgres or check host/port |
| "database does not exist" | DB not created | Run `createdb tiny-congress` |
| "extension pgmq does not exist" | Missing extension | Use provided Dockerfile.postgres |

## See also

- `service/src/config.rs` - Configuration struct and loading logic
- `service/config.yaml.example` - Example YAML configuration
- `service/src/main.rs` - Backend env var usage
- `kube/app/templates/deployment.yaml` - K8s configuration
- `dockerfiles/Dockerfile.postgres` - Database setup
- ADR-003: pgmq job queue

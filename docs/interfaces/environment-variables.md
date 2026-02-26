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
| `TC_DATABASE__HOST` | No | `localhost` | Database host |
| `TC_DATABASE__PORT` | No | `5432` | Database port |
| `TC_DATABASE__NAME` | No | `tiny-congress` | Database name |
| `TC_DATABASE__USER` | Yes | - | Database user |
| `TC_DATABASE__PASSWORD` | Yes | - | Database password |
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

### Security Headers Configuration

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `TC_SECURITY_HEADERS__ENABLED` | No | `true` | Enable security headers |
| `TC_SECURITY_HEADERS__HSTS_ENABLED` | No | `false` | Enable HSTS (use with HTTPS only) |
| `TC_SECURITY_HEADERS__HSTS_MAX_AGE` | No | `31536000` | HSTS max-age in seconds (1 year) |
| `TC_SECURITY_HEADERS__HSTS_INCLUDE_SUBDOMAINS` | No | `true` | Include subdomains in HSTS |
| `TC_SECURITY_HEADERS__FRAME_OPTIONS` | No | `DENY` | X-Frame-Options (`DENY` or `SAMEORIGIN`) |
| `TC_SECURITY_HEADERS__CONTENT_SECURITY_POLICY` | No | `default-src 'self'` | Content-Security-Policy header value |
| `TC_SECURITY_HEADERS__REFERRER_POLICY` | No | `strict-origin-when-cross-origin` | Referrer-Policy header value |

**Security note:** Security headers are enabled by default with safe values. HSTS is disabled by default since it requires HTTPS.

Headers applied (when enabled):
- `X-Content-Type-Options: nosniff`
- `X-Frame-Options: DENY` (configurable)
- `X-XSS-Protection: 1; mode=block`
- `Content-Security-Policy: default-src 'self'` (configurable)
- `Referrer-Policy: strict-origin-when-cross-origin` (configurable)
- `Strict-Transport-Security` (only if `hsts_enabled: true`)

Examples:
```bash
# Production with HSTS
TC_SECURITY_HEADERS__HSTS_ENABLED=true

# Allow iframes from same origin
TC_SECURITY_HEADERS__FRAME_OPTIONS=SAMEORIGIN

# Custom CSP for frontend compatibility
TC_SECURITY_HEADERS__CONTENT_SECURITY_POLICY="default-src 'self'; script-src 'self' 'unsafe-inline'"
```

### GraphQL Configuration

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `TC_GRAPHQL__PLAYGROUND_ENABLED` | No | `false` | Enable GraphQL Playground UI at `/graphql` (GET) |

**Security note:** GraphQL Playground is disabled by default for security. It exposes the full API schema and provides an interactive interface that could help attackers. Enable only in development environments.

Examples:
```bash
# Enable for local development
TC_GRAPHQL__PLAYGROUND_ENABLED=true
```

### Swagger UI Configuration

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `TC_SWAGGER__ENABLED` | No | `false` | Enable Swagger UI at `/swagger-ui` |

**Security note:** Swagger UI is disabled by default for security. It exposes REST API documentation. Enable only in development environments.

Examples:
```bash
# Enable for local development
TC_SWAGGER__ENABLED=true
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
  host: localhost
  port: 5432
  name: tiny-congress
  user: postgres
  password: postgres
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

Configuration is delivered to the API pod in two layers:

1. **ConfigMap** (`<release>-config`): mounted as `/etc/tc/config.yaml` — contains non-secret config (database host/port/name, logging, CORS, feature flags)
2. **Secret** (`<release>-database`): injected as `TC_DATABASE__USER` and `TC_DATABASE__PASSWORD` env vars — credentials only

Figment's priority order (defaults → yaml → env vars) means the env vars from the Secret override the ConfigMap's yaml values for credentials.

To use a pre-existing Secret (e.g., managed Postgres credentials):

```yaml
database:
  existingSecret: my-postgres-credentials  # must contain 'user' and 'password' keys
```

### Helm values reference

| Value | Default | Maps to |
|-------|---------|---------|
| `database.existingSecret` | `""` | Use pre-existing Secret (overrides chart-managed) |
| `database.host` | `""` (in-cluster Postgres) | `database.host` in ConfigMap |
| `database.port` | `5432` | `database.port` in ConfigMap |
| `database.name` | `tiny-congress` | `database.name` in ConfigMap |
| `database.user` | `postgres` | `user` key in Secret |
| `database.password` | `postgres` | `password` key in Secret |
| `logging.level` | `info` | `logging.level` in ConfigMap |
| `cors.allowedOrigins` | `""` | `cors.allowed_origins` in ConfigMap |
| `graphql.playgroundEnabled` | `false` | `graphql.playground_enabled` in ConfigMap |
| `swagger.enabled` | `false` | `swagger.enabled` in ConfigMap |

## Local Development

### Option 1: Environment variables

```bash
export TC_DATABASE__USER=postgres
export TC_DATABASE__PASSWORD=postgres
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
TC_DATABASE__USER=postgres TC_DATABASE__PASSWORD=postgres cargo run
```

## Database Connection

The backend assembles a PostgreSQL connection URL internally from individual config fields:

| Field | Config key | Env var | Default |
|-------|-----------|---------|---------|
| Host | `database.host` | `TC_DATABASE__HOST` | `localhost` |
| Port | `database.port` | `TC_DATABASE__PORT` | `5432` |
| Name | `database.name` | `TC_DATABASE__NAME` | `tiny-congress` |
| User | `database.user` | `TC_DATABASE__USER` | (required) |
| Password | `database.password` | `TC_DATABASE__PASSWORD` | (required) |

## Required Extensions

The database must have `pgmq` extension loaded:

```sql
CREATE EXTENSION IF NOT EXISTS pgmq;
```

This is handled automatically by `dockerfiles/Dockerfile.postgres`.

## Troubleshooting

| Error | Cause | Fix |
|-------|-------|-----|
| "database.user is required" | Missing TC_DATABASE__USER | Set `TC_DATABASE__USER` environment variable or configure in config.yaml |
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

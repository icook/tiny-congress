# TinyCongress API

A production-ready Rust web service scaffolding with GraphQL API.

## Architecture

The service uses:
- **Rust** with **Axum** for the web server
- **PostgreSQL** for the database
- **SQLx** for database interactions with compile-time query verification
- **Async-GraphQL** for the API layer

## Features

- GraphQL API with playground at `/graphql`
- Health check endpoint at `/health`
- Build info query for deployment verification
- Automatic database migrations with retry logic
- Structured logging with tracing
- CORS support
- Pedantic clippy lints for code quality

## Running the Service

### Prerequisites

- Rust toolchain
- PostgreSQL
- Docker and Docker Compose (for containerized development)
- Skaffold (for Kubernetes deployment)

### Environment Variables

The service requires the following environment variables:

- **DATABASE_URL** (required): PostgreSQL connection string with PGMQ extension
  - Example: `postgres://postgres:postgres@localhost:5432/tinycongress`
  - Must have `CREATE EXTENSION pgmq;` available

- **SESSION_SIGNING_KEY** (required): Secret key for signing JWT session tokens
  - Example: `openssl rand -base64 32` (generate a random 32-byte key)
  - Keep this secret and rotate regularly
  - For development: any string works, but use a proper secret in production

- **SESSION_SIGNING_KEY_OLD** (optional): Previous signing key for zero-downtime rotation
  - Set this to the old key when rotating to maintain existing sessions
  - Remove after all old tokens have expired

See `.env.example` for a complete list of environment variables.

### Setup

1. Create a PostgreSQL database:
```bash
createdb tinycongress
```

2. Install required extensions:
```sql
CREATE EXTENSION IF NOT EXISTS pgmq;
CREATE EXTENSION IF NOT EXISTS pgcrypto;
```

3. Set environment variables:
```bash
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/tinycongress
export SESSION_SIGNING_KEY=$(openssl rand -base64 32)
```

4. Run the server:
```bash
cargo run
```

The server will:
- Connect to PostgreSQL with retry logic (handles startup race conditions)
- Run database migrations automatically
- Start listening on port 8080 (or `PORT` env var)

### Database Migrations

This project uses SQLx for database migrations. Migrations are located in `service/migrations/` and define the schema for both the prioritization demo and the Phase 1 identity system.

**Running Migrations:**

```bash
# Set your database URL
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/tinycongress

# Run all pending migrations
cd service
sqlx migrate run
```

**Required PostgreSQL Extensions:**

The database requires the `pgcrypto` extension for UUID generation:

```sql
CREATE EXTENSION IF NOT EXISTS pgcrypto;
```

**Resetting the Database for Tests:**

To reset identity tables during development or testing:

```bash
# Drop and recreate all identity tables
psql $DATABASE_URL -c "DROP SCHEMA public CASCADE; CREATE SCHEMA public;"

# Re-run migrations
sqlx migrate run
```

For integration tests, the test suite automatically manages schema via `TRUNCATE` statements to ensure isolation between tests.

**Migration Files:**

- `01_init.sql` - Initial prioritization demo schema
- `02_identity_event_store.sql` - Sigchain event store for identity system
- `03_identity_read_models.sql` - Identity read models (accounts, devices, endorsements, etc.)
- `04_identity_rate_limits.sql` - Rate limiting tables for abuse controls

### Development with Skaffold

For local development with Kubernetes:

```bash
skaffold dev -p dev
```

This sets up a development environment with:
1. Local PostgreSQL database
2. Hot-reloading via file sync
3. Kubernetes deployment for realistic testing

### Running Tests

```bash
cargo test
```

For integration tests with a real database:
```bash
cargo test --features integration-tests
```

Or use Skaffold to run tests in containers:
```bash
skaffold build --file-output artifacts.json
skaffold test --build-artifacts artifacts.json
```

### Observability

The service exports Prometheus metrics at `/metrics` and provides a health check at `/health`.

**Metrics Endpoints:**

```bash
# Health check (includes DB connectivity)
curl http://localhost:8080/health

# Prometheus metrics
curl http://localhost:8080/metrics
```

**Available Metrics:**

- `auth.success` - Counter for successful authentications
- `auth.failure` - Counter for failed authentication attempts
- `device.revoked_attempt` - Counter for attempts to use revoked devices
- `endorsement.write` - Counter for endorsement creations
- `endorsement.revocation` - Counter for endorsement revocations
- `reducer.replay_seconds` - Histogram for reducer replay times

**Logging:**

Control log verbosity with the `RUST_LOG` environment variable:

```bash
# Info level (default)
export RUST_LOG=info

# Debug level for identity module
export RUST_LOG=info,tinycongress_api::identity=debug

# Trace level for all modules
export RUST_LOG=trace
```

All authentication events, rate limit violations, and security events are logged with structured tracing.

### Pre-push checklist

- From `/service`, run `cargo fmt` and `cargo clippy --all-targets --all-features -- -D warnings` before pushing.
- Run `RUST_TEST_THREADS=1 cargo test` against a local Postgres to mirror the integration flow used in CI.

## API Schema

The GraphQL API provides:

### Queries
- `buildInfo`: Get build metadata (version, git SHA, build time)

### Mutations
- `echo(message: String!)`: Echo back a message (placeholder)

Access the GraphQL Playground at `http://localhost:8080/graphql` for interactive exploration.

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `DATABASE_URL` | PostgreSQL connection string | `postgres://postgres:postgres@localhost:5432/tinycongress` |
| `PORT` | Server port | `8080` |
| `RUST_LOG` | Log level | `info` |
| `MIGRATIONS_DIR` | Custom migrations directory | `./migrations` |
| `APP_VERSION` | Application version for build info | `dev` |
| `GIT_SHA` | Git commit SHA for build info | `unknown` |
| `BUILD_TIME` | Build timestamp (RFC3339) | `unknown` |
| `BUILD_MESSAGE` | Optional build message | none |

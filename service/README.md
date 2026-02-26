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

### Setup

1. Create a PostgreSQL database:
```bash
createdb tiny-congress
```

2. Set environment variables:
```bash
export TC_DATABASE__USER=postgres
export TC_DATABASE__PASSWORD=postgres
```

3. Run the server:
```bash
cargo run
```

The server will:
- Connect to PostgreSQL with retry logic (handles startup race conditions)
- Run database migrations automatically
- Start listening on port 8080 (or `TC_SERVER__PORT` env var)

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

Tests that need a database use [testcontainers](https://testcontainers.com/) to automatically
spin up an isolated PostgreSQL container. First build the custom postgres image:

```bash
# One-time setup (from repo root)
just build-test-postgres
```

Or use Skaffold to run tests in containers:
```bash
skaffold build --file-output artifacts.json
skaffold test --build-artifacts artifacts.json
```

## API Schema

The GraphQL API provides:

### Queries
- `buildInfo`: Get build metadata (version, git SHA, build time)

### Mutations
- `echo(message: String!)`: Echo back a message (placeholder)

Access the GraphQL Playground at `http://localhost:8080/graphql` for interactive exploration.

## Environment Variables

Configuration is loaded via [Figment](https://docs.rs/figment/) with `TC_`-prefixed env vars (double underscore `__` separates nesting levels). Env vars override `config.yaml` values.

| Variable | Description | Default |
|----------|-------------|---------|
| `TC_DATABASE__HOST` | Database host | `localhost` |
| `TC_DATABASE__PORT` | Database port | `5432` |
| `TC_DATABASE__NAME` | Database name | `tiny-congress` |
| `TC_DATABASE__USER` | Database user (required) | — |
| `TC_DATABASE__PASSWORD` | Database password (required) | — |
| `TC_DATABASE__MAX_CONNECTIONS` | Connection pool size | `10` |
| `TC_DATABASE__MIGRATIONS_DIR` | Custom migrations directory | none |
| `TC_SERVER__PORT` | Server port | `8080` |
| `TC_SERVER__HOST` | Bind address | `0.0.0.0` |
| `TC_LOGGING__LEVEL` | tracing filter directive (e.g. `debug`, `info`, `warn`) | `info` |
| `TC_CORS__ALLOWED_ORIGINS` | Comma-separated origins or `*` | none |
| `TC_GRAPHQL__PLAYGROUND_ENABLED` | Enable GraphQL Playground at `/graphql` | `false` |
| `TC_SWAGGER__ENABLED` | Enable Swagger UI at `/swagger-ui` | `false` |
| `TC_SECURITY_HEADERS__ENABLED` | Enable security response headers | `true` |
| `APP_VERSION` | Application version for build info | `dev` |
| `GIT_SHA` | Git commit SHA for build info | `unknown` |
| `BUILD_TIME` | Build timestamp (RFC3339) | `unknown` |
| `BUILD_MESSAGE` | Optional build message | none |

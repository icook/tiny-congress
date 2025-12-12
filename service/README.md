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
export DATABASE_URL=postgres://username:password@localhost/tiny-congress
```

3. Run the server:
```bash
cargo run
```

The server will:
- Connect to PostgreSQL with retry logic (handles startup race conditions)
- Run database migrations automatically
- Start listening on port 8080 (or `PORT` env var)

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
| `DATABASE_URL` | PostgreSQL connection string | `postgres://postgres:postgres@localhost:5432/tiny-congress` |
| `PORT` | Server port | `8080` |
| `RUST_LOG` | Log level | `info` |
| `MIGRATIONS_DIR` | Custom migrations directory | `./migrations` |
| `APP_VERSION` | Application version for build info | `dev` |
| `GIT_SHA` | Git commit SHA for build info | `unknown` |
| `BUILD_TIME` | Build timestamp (RFC3339) | `unknown` |
| `BUILD_MESSAGE` | Optional build message | none |

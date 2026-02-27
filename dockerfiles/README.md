# Dockerfiles

Custom Docker images for the TinyCongress development and CI environment.

## Files

| Dockerfile | Purpose | Base Image |
|------------|---------|------------|
| `Dockerfile.postgres` | PostgreSQL with pgmq extension | `postgres:18` |

## Dockerfile.postgres

Custom PostgreSQL image that includes the [pgmq](https://github.com/tembo-io/pgmq) extension for job queue functionality.

**Build:** Built automatically on first `just test-backend` run. To rebuild manually:
```bash
docker build -t tc-postgres:local -f dockerfiles/Dockerfile.postgres dockerfiles/
```

**Key features:**
- PostgreSQL 18
- pgmq extension pre-installed
- Default database: `tiny-congress`
- Default credentials: `postgres:postgres`

## Service Dockerfiles

The main API Dockerfiles live in `service/`:
- `service/Dockerfile` - Production API image
- `service/Dockerfile.dev` - Development API image with hot reload

These use cargo-chef for optimized layer caching. See [ADR-001](../docs/decisions/001-cargo-chef-docker-builds.md).

## UI Dockerfiles

The web client Dockerfiles live in `web/`:
- `web/Dockerfile` - Production UI image (Nginx-served static build)
- `web/Dockerfile.dev` - Development UI image with Vite hot reload

## Why Dockerfiles live in different directories

We intentionally keep dev and prod Dockerfiles close to their code, while shared
infrastructure lives in `dockerfiles/`:
- Dev Dockerfiles (`*.dev`) use subdirectory build contexts with `COPY . .` so
  Skaffold file sync and hot reload work naturally.
- Prod Dockerfiles use the repo root context to pull shared crates and other
  cross-project dependencies explicitly.
- The Postgres image is shared infra, so it stays centralized under
  `dockerfiles/`.

## Related

- [docker-layer-caching playbook](../docs/playbooks/docker-layer-caching.md)
- [skaffold.yaml](../skaffold.yaml) - Image build orchestration

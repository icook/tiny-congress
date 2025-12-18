# Dockerfiles

Custom Docker images for the TinyCongress development and CI environment.

## Files

| Dockerfile | Purpose | Base Image |
|------------|---------|------------|
| `Dockerfile.postgres` | PostgreSQL with pgmq extension | `postgres:17` |

## Dockerfile.postgres

Custom PostgreSQL image that includes the [pgmq](https://github.com/tembo-io/pgmq) extension for job queue functionality.

**Build:**
```bash
just build-test-postgres
```

**Key features:**
- PostgreSQL 17
- pgmq extension pre-installed
- Default database: `tiny-congress`
- Default credentials: `postgres:postgres`

## Service Dockerfiles

The main application Dockerfiles live in `service/`:
- `service/Dockerfile` - Production API image
- `service/Dockerfile.dev` - Development API image with hot reload

These use cargo-chef for optimized layer caching. See [ADR-001](../docs/decisions/001-cargo-chef-docker-builds.md).

## Related

- [docker-layer-caching playbook](../docs/playbooks/docker-layer-caching.md)
- [skaffold.yaml](../skaffold.yaml) - Image build orchestration

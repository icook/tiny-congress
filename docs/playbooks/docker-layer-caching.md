# Docker Layer Caching

## When to use
- Debugging slow Docker builds
- Modifying Dockerfiles
- Optimizing CI build times

## Architecture

### Rust service (cargo-chef pattern)

```dockerfile
# Stage 1: Planner - compute recipe
FROM rust AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Stage 2: Cacher - build dependencies only
FROM rust AS cacher
COPY --from=planner recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Stage 3: Builder - build application
FROM rust AS builder
COPY --from=cacher /app/target target
COPY . .
RUN cargo build --release
```

**Why this works:**
- `recipe.json` only changes when `Cargo.toml`/`Cargo.lock` change
- Dependency compilation cached until recipe changes
- Source changes only rebuild application code

### Frontend (multi-stage)

```dockerfile
# Stage 1: Dependencies
FROM node AS deps
COPY package.json yarn.lock ./
RUN yarn install --frozen-lockfile

# Stage 2: Build
FROM node AS builder
COPY --from=deps /app/node_modules ./node_modules
COPY . .
RUN yarn build

# Stage 3: Production
FROM nginx AS runner
COPY --from=builder /app/dist /usr/share/nginx/html
```

## Cache sources (CI)

CI uses multiple cache layers:
1. **GitHub Actions cache** (`type=gha`) - Fast, per-workflow
2. **Registry cache** (`type=registry`) - Shared across workflows

```yaml
cache-from: |
  type=gha,scope=${{ matrix.image }}
  type=registry,ref=${{ env.REGISTRY }}/${{ matrix.image }}:cache
cache-to: |
  type=gha,scope=${{ matrix.image }},mode=max
  type=registry,ref=${{ env.REGISTRY }}/${{ matrix.image }}:cache,mode=max
```

## Cache invalidation triggers

| Change | Invalidates |
|--------|-------------|
| `Cargo.toml` / `Cargo.lock` | Rust dependency layer |
| `package.json` / `yarn.lock` | Node dependency layer |
| Dockerfile changes | All layers after change |
| Base image update | Entire build |

## Debugging cache misses

### Check what's being cached
```bash
docker buildx build --progress=plain . 2>&1 | grep -E "(CACHED|RUN)"
```

### Force rebuild without cache
```bash
docker buildx build --no-cache .
```

### Inspect layer sizes
```bash
docker history <image>
```

## Optimization tips

1. **Order matters**: Put rarely-changing steps first
2. **Minimize COPY scope**: Copy only what's needed per stage
3. **Use .dockerignore**: Exclude `target/`, `node_modules/`, etc.
4. **Pin base images**: Use specific tags, not `latest`

## Verification
- [ ] Local build uses cache: `CACHED` in output
- [ ] CI build times reasonable (<10min for unchanged deps)
- [ ] No unnecessary cache invalidation

## Common failures

| Error | Cause | Fix |
|-------|-------|-----|
| Cache miss every build | Layer ordering wrong | Reorder Dockerfile |
| "no space left" | Cache too large | Prune old layers |
| "failed to fetch" | Registry cache unavailable | Falls back to GHA cache |

## Prohibited actions
- DO NOT remove cargo-chef stages from Rust Dockerfiles
- DO NOT use `COPY . .` before dependency installation

## See also
- `service/Dockerfile` - Rust build
- `web/Dockerfile` - Frontend build
- `.github/workflows/ci.yml` - Cache configuration

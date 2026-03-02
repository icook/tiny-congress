# ADR-014: CI Pipeline Architecture

## Status
Accepted

## Context

TinyCongress's CI must build three container images (API, frontend, custom Postgres), run backend and frontend unit tests with coverage, deploy to a Kubernetes cluster, run E2E tests against the running system, and gate on security scans — all within a reasonable time budget on GitHub Actions runners.

Several tensions shaped this decision:

- **Build once vs. build per stage.** Rebuilding images for each CI stage (test, scan, deploy) wastes time and risks non-reproducible builds. Building once and reusing via image tags is faster but requires orchestrating artifacts between jobs.
- **Tool version consistency.** Rust, Node, and other tool versions must match across local development, Docker builds, and CI. Drift between these causes "works on my machine" failures.
- **Cluster startup latency.** KinD cluster creation takes 60–90 seconds. Waiting for the cluster before starting any work wastes the entire window. Starting it in the background lets other work proceed in parallel.

## Decision

### Images built once, tagged with SHA, reused via artifacts file

Three images are built in a single `build-images` matrix job and pushed to GHCR:

| Image | Dockerfile | Purpose |
|-------|-----------|---------|
| `tc-api-release` | `service/Dockerfile` | Rust API server |
| `tc-ui-release` | `web/Dockerfile` | React frontend on Nginx |
| `postgres` | `dockerfiles/postgres/Dockerfile` | PostgreSQL 18 with pgmq extension |

Each image gets two tags:
- **SHA tag** (immutable): `ghcr.io/icook/tiny-congress/<image>:<github.sha>`
- **Branch tag** (mutable): `ghcr.io/icook/tiny-congress/<image>:branch-<sanitized-branch>`

Downstream jobs receive a Skaffold artifacts JSON file that maps image names to SHA-tagged references:

```json
{
  "builds": [
    {"imageName": "tc-api-release", "tag": "ghcr.io/icook/tiny-congress/tc-api-release:<sha>"},
    {"imageName": "tc-ui-release", "tag": "ghcr.io/icook/tiny-congress/tc-ui-release:<sha>"},
    {"imageName": "postgres", "tag": "ghcr.io/icook/tiny-congress/postgres:<sha>"}
  ]
}
```

Skaffold's `--build-artifacts` flag consumes this file, deploying the pre-built images without rebuilding.

### Tool versions from `mise.toml` as Docker build-args

`mise.toml` is the single source of truth for tool versions:

```toml
[tools]
node = "22"
rust = "1.91.0"
```

CI extracts these values and passes them as Docker build arguments:

```yaml
build-args: |
  RUST_VERSION=${{ steps.versions.outputs.rust }}
  NODE_VERSION=${{ steps.versions.outputs.node }}
  GIT_SHA=${{ github.sha }}
  APP_VERSION=${{ github.ref_name }}@${{ github.sha }}
  BUILD_TIME=${{ github.event.head_commit.timestamp }}
```

Dockerfiles declare matching `ARG` defaults:

```dockerfile
ARG RUST_VERSION=1.91.0
FROM lukemathwalker/cargo-chef:latest-rust-${RUST_VERSION} AS chef
```

The backend Dockerfile uses cargo-chef's three-stage build (planner → cacher → builder) for dependency layer caching. The frontend Dockerfile includes a WASM build stage (`rust:${RUST_VERSION}-slim` for wasm-pack) before the Node builder stage.

### KinD cluster started in background

The `integration-tests` job starts the KinD cluster immediately without waiting for it:

```yaml
- name: Start KinD cluster (background)
  run: |
    kind create cluster --name skaffold-ci --wait 0s &
```

While the cluster initializes (~60–90s), the job proceeds with:
1. Installing and caching Skaffold
2. Setting up Rust toolchain with `llvm-tools-preview`
3. Installing `cargo-llvm-cov`
4. Setting up Node and caching dependencies
5. Installing Playwright browsers
6. Building WASM module
7. Running Skaffold tests (unit tests via `cargo llvm-cov` and `yarn vitest:coverage`)

The cluster is only needed for deployment and E2E tests, which run after unit tests. A readiness check with 180-second timeout polls `kubectl cluster-info` before proceeding to deploy.

### Frontend runtime config injection via `docker-entrypoint.sh`

The frontend Docker image uses an entrypoint script (~30 lines) that generates `/usr/share/nginx/html/config.js` at container startup:

```bash
# Validates VITE_API_URL starts with http:// or https://
# Generates: window.__TC_ENV__ = { VITE_API_URL: "...", TC_ENVIRONMENT: "..." };
```

This enables one image for all environments — the same `tc-ui-release` image runs in CI, staging, and production with different `VITE_API_URL` values. Nginx serves `config.js` with `Cache-Control: no-store` to ensure environment changes take effect without cache invalidation.

In Kubernetes, the values come from Helm chart values (`frontend.apiUrl`, `frontend.environment`) injected as container environment variables.

### Trivy scan gates integration tests

The `scan-images` job runs Trivy against all three images before integration tests can proceed:

```yaml
- uses: aquasecurity/trivy-action@v0.33.1
  with:
    image-ref: ghcr.io/icook/tiny-congress/<image>:<sha>
    exit-code: '1'
    ignore-unfixed: true
    severity: 'CRITICAL,HIGH'
    trivyignores: '.trivyignore'
```

Configuration:
- **Severity gate:** CRITICAL and HIGH only
- **Unfixed CVEs:** Ignored (only actionable findings block the build)
- **`.trivyignore`:** Exempts known-acceptable CVEs (e.g., Postgres `gosu` binary vulnerabilities that are only used during container startup to drop privileges)

The `integration-tests` job depends on `scan-images` completing successfully — a critical vulnerability blocks deployment to the test cluster.

### 60% backend line coverage threshold

Backend coverage uses `cargo-llvm-cov` with a hard minimum:

```bash
cargo llvm-cov --lcov \
  --output-path coverage/backend-unit.lcov \
  --fail-under-lines 60
```

The job fails if backend line coverage drops below 60%. This threshold is intentionally conservative — it catches large regressions without forcing coverage-driven test writing.

Frontend coverage thresholds are configured in Vite: 70% for statements/functions/lines, 60% for branches.

Coverage reports from all three test types (Rust unit, Vitest, Playwright) are aggregated into a unified HTML report and deployed to Cloudflare Pages.

### E2E via Playwright against port-forwarded KinD cluster

After deploying to KinD, the pipeline runs Playwright tests against the live system:

1. **Port-forward services:**
   ```bash
   kubectl port-forward service/tc 8080:8080 &
   ```

2. **Serve instrumented frontend:** The frontend is rebuilt with `PLAYWRIGHT_COVERAGE=1` for Istanbul coverage instrumentation, then served via `yarn preview` on port 5173.

3. **Health check:** Polls `http://localhost:8080/health` with 60-second timeout.

4. **Run tests:**
   ```bash
   PLAYWRIGHT_BASE_URL=http://localhost:5173 \
   PLAYWRIGHT_API_URL=http://localhost:8080/graphql \
   yarn playwright:test
   ```

5. **Post-test:** Flakiness analysis runs via `scripts/analyze-playwright-flakiness.mjs` and results are added to the GitHub Actions job summary.

Playwright test results are published as check annotations via the `publish-unit-test-result-action`.

### Job dependency chain

```
Parallel (no dependencies):
  lint, lint-web, build-storybook, build-docs, codegen-check, security-audit

build-images (after parallel jobs)
  ↓
scan-images + sqlx-check (after build-images)
  ↓
integration-tests (after all above)
  ↓
deploy-gitops (master only) + deploy-cloudflare
```

The `integration-tests` job depends on all preceding jobs. This ensures that linting, security audits, type checking, codegen freshness, and image scanning all pass before consuming cluster resources.

## Consequences

### Positive
- Images are built exactly once per commit. Downstream jobs deploy the same bytes that were scanned and tested.
- Background KinD startup overlaps with ~5 minutes of lint, build, and unit test work, hiding most of the cluster startup latency.
- `mise.toml` as single source of truth for tool versions eliminates version drift between local dev, Docker builds, and CI.
- Trivy scanning before integration tests catches critical vulnerabilities before code runs in a cluster.
- Runtime config injection means the frontend image works in any environment without rebuilding.
- Coverage aggregation across three test types (Rust, Vitest, Playwright) gives a unified view of test coverage.

### Negative
- The Skaffold artifacts JSON is generated in CI from hardcoded image names — adding a new image requires updating both `skaffold.yaml` and the CI workflow.
- `cargo-llvm-cov` adds ~30s to the test phase compared to running tests without coverage instrumentation.
- Port-forwarding for E2E tests is fragile — if a service fails to start, the health check timeout is 60 seconds of wasted time before failure.
- The 60% backend coverage threshold is a blunt instrument. It catches large regressions but doesn't prevent coverage erosion in specific modules.

### Neutral
- Playwright browsers are cached by `yarn.lock` hash. Browser version updates (via `yarn upgrade`) invalidate the cache and trigger a fresh download (~300MB).
- The `deploy-cloudflare` job runs even if integration tests fail (with `always()` condition), ensuring coverage reports and Storybook are always deployed for debugging.
- Storybook is built in parallel with other lint jobs but only consumed by the final Cloudflare deployment.

## Alternatives considered

### Build images in each job that needs them
- Simpler workflow — no artifact passing, no GHCR push during CI
- Rejected because rebuilding the same images 3+ times per CI run wastes ~15 minutes and risks non-reproducible builds (dependencies could change between builds)

### Docker Compose instead of KinD for integration tests
- Simpler — no Kubernetes, no Skaffold, no cluster startup
- Rejected because it doesn't test Kubernetes-specific behavior (ConfigMap mounting, Secret injection, health probes, service discovery). KinD provides CI parity with production Kubernetes deployments.

### Hardcoded tool versions in Dockerfiles and CI
- Simpler — no extraction step, no `mise.toml` dependency
- Rejected because version drift is a real problem. When Rust version differs between local dev and CI, compilation behavior and dependency resolution can change. A single source of truth prevents this.

### Coverage thresholds per module instead of global
- More precise — catches coverage erosion in specific areas
- Rejected for now as premature. The global 60% threshold catches large regressions. Per-module thresholds can be added when coverage is more stable and patterns are established.

### Playwright against Docker Compose instead of KinD
- Faster E2E setup — no cluster, no Helm deployment
- Rejected for the same reason as Docker Compose above: the E2E tests should exercise the same deployment topology as production.

## References
- [ADR-001: Cargo-Chef Docker Builds](001-cargo-chef-docker-builds.md) — backend Docker build strategy
- [ADR-002: Skaffold Orchestration](002-skaffold-orchestration.md) — Skaffold profiles and deployment
- [ADR-013: Frontend Architecture](013-frontend-architecture.md) — `window.__TC_ENV__` pattern consumed by the entrypoint
- [PR #274: CI pipeline overhaul](https://github.com/icook/tiny-congress/pull/274) — build-once strategy
- [PR #268: Coverage and E2E integration](https://github.com/icook/tiny-congress/pull/268) — coverage thresholds and Playwright
- `.github/workflows/ci.yml` — CI workflow definition
- `web/docker-entrypoint.sh` — frontend runtime config injection
- `mise.toml` — tool version definitions
- `skaffold.yaml` — Skaffold profiles and artifact configuration
- `.trivyignore` — exempted CVEs with rationale

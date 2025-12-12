# Debugging CI Failures

## When to use
- CI pipeline failed on your PR
- Need to reproduce CI behavior locally

## Quick diagnosis

### 1. Identify the failing job
```bash
gh run view --web                    # Open in browser
gh run watch                         # Stream live logs
gh run view <run_id> --log-failed    # Show only failed steps
```

### 2. Common failure patterns

| Job | Common cause | Quick fix |
|-----|--------------|-----------|
| `lint` | Formatting | `cd service && cargo fmt` |
| `lint-web` | ESLint/TypeScript | `cd web && yarn lint --fix` |
| `build-images` | Dockerfile error | Check build context paths |
| `integration-tests` | Test failure | See test output for assertion |
| `agent-compliance` | Missing YAML block | Add compliance block to PR |

## Local reproduction

### Rust lint failures
```bash
cd service
cargo fmt --all -- --check          # Check formatting
cargo clippy --all-features -- -D warnings
```

### Web lint failures
```bash
cd web
yarn lint                           # ESLint + Stylelint
yarn typecheck                      # TypeScript
yarn prettier --check .             # Formatting
```

### Integration test failures
```bash
# Full CI simulation
skaffold build --file-output artifacts.json
skaffold test -p ci --build-artifacts artifacts.json
skaffold deploy -p ci --build-artifacts artifacts.json

# Port-forward and run tests
kubectl port-forward svc/postgres 5432:5432 &
cd service && DATABASE_URL=postgres://postgres:postgres@localhost:5432/prioritization cargo test
```

### Playwright E2E failures
```bash
kubectl port-forward deployment/tc-frontend 5173:80 &
cd web && PLAYWRIGHT_BASE_URL=http://localhost:5173 yarn playwright:test
```

## Reading CI logs

### Key sections to check
1. **Error summary** - Usually at end of failed step
2. **Test output** - Look for `FAILED` or `Error:`
3. **kubectl events** - If pods failing: `kubectl get events`

### Artifacts to download
- `playwright-artifacts` - Screenshots, traces, coverage
- `rust-coverage` - LCOV files for coverage analysis

```bash
gh run download <run_id> -n playwright-artifacts
```

## Verification after fix
- [ ] Failure reproduces locally
- [ ] Fix applied
- [ ] `skaffold test -p ci` passes locally
- [ ] Push and verify CI passes

## Common failures

| Error | Cause | Fix |
|-------|-------|-----|
| "KinD not ready" | Cluster timeout | Retry or check resource limits |
| "image pull failed" | Registry auth | Check GHCR token permissions |
| "connection refused" | Port-forward died | Restart port-forward |
| Flaky test | Race condition | Check test isolation |

## See also
- `.github/workflows/ci.yml` - CI workflow definition
- `skaffold.yaml` - Build/deploy configuration
- AGENTS.md § Build, Test, and Development Commands

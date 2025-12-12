# Skaffold Profiles

## When to use
- Running local development
- Testing CI pipeline locally
- Debugging deployment issues

## Available profiles

| Profile | Purpose | Command |
|---------|---------|---------|
| `dev` | Local development with hot reload | `skaffold dev -p dev` |
| `ci` | CI pipeline simulation | `skaffold test -p ci && skaffold verify -p ci` |
| (default) | Production-like build | `skaffold build` |

## Common workflows

### Local development
```bash
skaffold dev -p dev
```
- Watches for file changes
- Rebuilds and redeploys automatically
- Streams logs to terminal

### Testing locally before PR
```bash
# Build images
skaffold build --file-output artifacts.json

# Run container tests
skaffold test --build-artifacts artifacts.json

# Deploy and run integration tests
skaffold deploy -p ci --build-artifacts artifacts.json
skaffold verify -p ci --build-artifacts artifacts.json
```

### Reusing pre-built images
```bash
# If images already built (e.g., from CI)
skaffold test -p ci --build-artifacts artifacts.json
skaffold verify -p ci --build-artifacts artifacts.json
```

## Image naming

| Image | Description |
|-------|-------------|
| `tc-api-release` | Rust API server (production build) |
| `tc-ui-release` | React frontend (nginx serving built assets) |
| `postgres` | PostgreSQL with pgmq extension |

## Verification
- [ ] `skaffold diagnose` shows no errors
- [ ] Images build successfully
- [ ] Pods reach Running state

## Common failures

| Error | Cause | Fix |
|-------|-------|-----|
| "image not found" | Wrong registry/tag | Check `SKAFFOLD_DEFAULT_REPO` |
| Pod CrashLoopBackOff | App startup failure | Check `kubectl logs <pod>` |
| Port conflict | Service already bound | Kill existing port-forward |

## Prohibited actions
- DO NOT modify `skaffold.yaml` profiles without running testing-local-dev skill
- DO NOT change image names without updating CI workflow

## See also
- `skaffold.yaml` - profile definitions
- `kube/app/` - Helm chart templates
- AGENTS.md § Testing Guidelines

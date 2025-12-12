# ADR-002: Use Skaffold for build and deploy orchestration

## Status
Accepted

## Context
The project has multiple container images (API, frontend, Postgres) that need to be built, tested, and deployed together. We need:
- Local development with hot reload
- CI pipeline that mirrors production
- Consistent image tagging across environments

Managing this with raw Docker/kubectl commands is error-prone and inconsistent.

## Decision
Use [Skaffold](https://skaffold.dev/) to orchestrate builds, tests, and deployments across all environments.

Configuration in `skaffold.yaml` defines:
- Build artifacts (images and their Dockerfiles)
- Test steps (container structure tests)
- Deploy targets (Helm charts)
- Profiles for different environments (dev, ci)

## Consequences

### Positive
- Single command for full stack: `skaffold dev`, `skaffold run`
- Profile-based configuration eliminates environment drift
- Built-in file watching for development
- Artifact passing between CI stages (`--build-artifacts`)
- Consistent image tagging

### Negative
- Another tool to learn
- Skaffold version must be pinned (breaking changes between versions)
- Some edge cases require workarounds

### Neutral
- Helm charts remain portable (Skaffold just orchestrates them)
- Can still use raw kubectl when needed

## Alternatives considered

### Docker Compose
- Simpler for local development
- Rejected: Doesn't match Kubernetes deployment model, would need separate CI config

### Tilt
- Similar to Skaffold with better UI
- Rejected: less opinionated/batteries included (and Python)

### Raw Makefile + kubectl
- Maximum flexibility
- Rejected: Too much custom scripting, easy to diverge between environments

## References
- Skaffold documentation: https://skaffold.dev/docs/
- `skaffold.yaml` - configuration
- `../playbooks/skaffold-profiles.md` - usage guide

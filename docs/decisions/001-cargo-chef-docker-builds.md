# ADR-001: Use cargo-chef for Rust Docker builds

## Status
Accepted

## Context
Rust Docker builds are slow because `cargo build` recompiles all dependencies when any source file changes. In CI, this means 5-10 minute builds even for one-line changes. Docker layer caching doesn't help because the entire `cargo build` step invalidates when source changes.

## Decision
Use [cargo-chef](https://github.com/LukeMathWalker/cargo-chef) to separate dependency compilation from application compilation in Docker builds.

The build uses three stages:
1. **Planner**: Generates `recipe.json` from `Cargo.toml` and `Cargo.lock`
2. **Cacher**: Builds dependencies only using the recipe
3. **Builder**: Copies cached dependencies and builds application

## Consequences

### Positive
- Dependency layer cached until `Cargo.toml`/`Cargo.lock` change
- Source-only changes rebuild in ~1 minute instead of ~8 minutes
- CI costs reduced significantly

### Negative
- More complex Dockerfile (three stages instead of one)
- Requires understanding cargo-chef when debugging build issues
- Additional Docker image layers

### Neutral
- Same final binary output
- No runtime impact

## Alternatives considered

### sccache
- Shared compilation cache across builds
- Rejected: Requires cache server infrastructure, more operational complexity

### cargo-cache action
- GitHub Actions cache for target directory
- Rejected: Cache size limits (10GB), cache invalidation unreliable

## References
- cargo-chef documentation: https://github.com/LukeMathWalker/cargo-chef
- `service/Dockerfile` - implementation
- `doc/playbooks/docker-layer-caching.md` - usage guide

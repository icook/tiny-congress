# CI Linting Enhancements Design

**Issue**: #244 - Add kube-linter and rustdoc warnings to CI
**Date**: 2025-12-20
**Status**: Approved

## Overview

Add two linting tools to improve code quality and catch issues early:
1. **kube-linter** - Static analysis for Kubernetes manifests (blocking)
2. **rustdoc warnings** - Documentation quality checks including missing docs

Note: cargo-outdated was originally in scope but removed as redundant with existing Dependabot configuration.

## 1. kube-linter Integration

### CI Job

New `lint-kube` job in `.github/workflows/ci.yml`, running parallel with other lint jobs:

```yaml
lint-kube:
  name: Lint Kubernetes manifests
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v6
    - uses: stackrox/kube-linter-action@v1
      with:
        directory: kube/
        config: .kube-linter.yaml
```

### Configuration

New `.kube-linter.yaml` at repo root with sensible defaults. Exclude checks that false-positive on Helm template variables (e.g., image tags set by Skaffold).

### Just Recipe

```just
lint-kube:
    kube-linter lint kube/ --config .kube-linter.yaml
```

Keep separate from main `lint` recipe since kube-linter requires installation.

## 2. Rustdoc Warnings

### Workspace Lints

Add to `Cargo.toml`:

```toml
[workspace.lints.rust]
missing_docs = "warn"

[workspace.lints.rustdoc]
broken_intra_doc_links = "warn"
private_intra_doc_links = "warn"
```

### CI Integration

Warnings surface during existing `lint-backend` job. No new job needed.

### Just Recipe

```just
lint-docs:
    cd service && cargo doc --no-deps --document-private-items 2>&1 | grep -E "warning:" && exit 1 || exit 0
```

## 3. Documentation Fixes

39 missing doc warnings to fix across 7 files:

| File | Count | Items |
|------|-------|-------|
| `service/src/config.rs` | 16 | Config struct fields |
| `service/src/identity/repo/accounts.rs` | 6 | Repository types |
| `service/src/lib.rs` | 5 | Module re-exports |
| `service/src/identity/http/mod.rs` | 5 | HTTP handler types |
| `service/src/build_info.rs` | 5 | Build metadata |
| `service/src/main.rs` | 1 | Crate root |
| `crates/test-macros/src/lib.rs` | 1 | Macro crate |

### Documentation Style

Keep doc comments minimal to avoid duplication with `docs/`:

```rust
/// Database connection pool configuration.
pub struct DbConfig {
    /// PostgreSQL connection URL.
    pub url: String,
    /// Maximum connections in pool.
    pub max_connections: u32,
}
```

If detailed docs exist in `docs/`, reference them:
```rust
/// Server configuration. See docs/playbooks/local-dev-setup.md for details.
```

## 4. Implementation Checklist

- [ ] Create `.kube-linter.yaml` with initial config
- [ ] Run kube-linter locally and fix any issues
- [ ] Add `lint-kube` job to CI workflow
- [ ] Add `[workspace.lints.rust]` and `[workspace.lints.rustdoc]` to Cargo.toml
- [ ] Add docs to `service/src/config.rs` (16 items)
- [ ] Add docs to `service/src/identity/repo/accounts.rs` (6 items)
- [ ] Add docs to `service/src/lib.rs` (5 items)
- [ ] Add docs to `service/src/identity/http/mod.rs` (5 items)
- [ ] Add docs to `service/src/build_info.rs` (5 items)
- [ ] Add docs to `service/src/main.rs` (1 item)
- [ ] Add docs to `crates/test-macros/src/lib.rs` (1 item)
- [ ] Add `lint-kube` and `lint-docs` recipes to justfile
- [ ] Update CLAUDE.md with new commands
- [ ] Verify `just lint` and CI pass

# Ticket: Release Hygiene Checks

## Goal

Keep dependencies and secrets clean with blocking CI enforcement — treating security and dependency hygiene like linting.

## Acceptance Criteria

| Check | Tool | Blocking | Threshold |
|-------|------|----------|-----------|
| Rust vulnerability advisories | `cargo deny` | Yes | HIGH and CRITICAL |
| Rust license compliance | `cargo deny` | Yes | Deny GPL/AGPL/unlicensed |
| Rust unused dependencies | `cargo machete` | Yes | Any unused = fail |
| Frontend vulnerabilities | `yarn audit` | Yes | `--level high` |
| Secret detection | `gitleaks` | Yes | Any finding = fail |

## Implementation Tasks

### 1. Add `cargo-deny` configuration

Create `service/deny.toml` with:
- Advisory database checks (block HIGH+)
- License allowlist (MIT, Apache-2.0, BSD, ISC, MPL-2.0, Zlib, Unicode-3.0)
- Deny copyleft licenses in production deps

### 2. Add CI job

Add `security-audit` job to `.github/workflows/ci.yml`:
- `cargo deny check` for advisories + licenses
- `cargo machete` for unused dependencies
- `yarn audit --level high` for frontend
- `gitleaks` for secret detection

### 3. Add justfile recipes

Add local commands for developers:
- `just audit` — run all hygiene checks
- `just audit-deps` — cargo deny + yarn audit
- `just audit-secrets` — gitleaks
- `just audit-unused` — cargo machete

### 4. Update README.md

Add Security section documenting:
- Required tools installation
- How to run checks locally
- What CI enforces

## Files Changed

| File | Action |
|------|--------|
| `.github/workflows/ci.yml` | Add `security-audit` job |
| `service/deny.toml` | New file |
| `justfile` | Add audit recipes |
| `README.md` | Add Security section |

## Out of Scope (Future Tickets)

- Container image scanning (Trivy)
- SBOM generation
- Dependabot auto-PR configuration
- Pre-commit hook enforcement

## Branch

`ci/release-hygiene-checks`

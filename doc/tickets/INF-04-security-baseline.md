# INF-04 Security baseline

Goal: tighten dependency scanning, SAST, and backups for Phase 1 identity features.

Deliverables
- Dependency scanning configured for Rust and Web packages.
- SAST/static checks integrated into CI.
- Backup/PITR guidance for Postgres.

Implementation plan
1) Dependency scanning: add `cargo audit` (or `cargo deny`) step to CI workflow and `yarn audit --environment production` (or `npm audit --omit=dev` via yarn) for web. Document how to run locally.

2) SAST: enable `cargo clippy --all-targets -- -D warnings` and `cargo fmt -- --check` in CI if not already. For web, ensure `yarn lint` covers ESLint/Prettier/Stylelint (already part of `yarn test`). Consider adding `semgrep` rules for auth endpoints; if added, document config location.

3) Backups/PITR: write guidance in `doc/verify-artifacts.md` or new `doc/skills/backup.md` on configuring Postgres backups and point-in-time recovery. Include `pg_dump` instructions for dev.

4) Secrets scanning: optionally add `gitleaks` or similar in CI to catch committed secrets (ties to INF-02). Ensure ignores are tuned to not flag test vectors.

5) Infra manifests: review `skaffold.yaml` and `kube/` assets to ensure new env vars (session key) are set via secrets, not plain env in deployment yaml. Add notes where to update if not applying now.

Verification
- Run CI locally: `cargo fmt -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo audit`, `cd web && yarn audit && yarn lint`.
- Confirm documentation exists for backup plan and is referenced from README.

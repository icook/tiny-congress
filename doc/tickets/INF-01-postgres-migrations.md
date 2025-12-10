# INF-01 Postgres + migrations

Goal: add Phase 1 identity schema via SQL migrations and ensure Skaffold/CI runs them.

Deliverables
- New migrations under `service/migrations/` defining identity/event-store tables and indexes.
- Local + CI paths that apply migrations automatically (via existing `db::setup_database` and skaffold pipeline).
- Safety checks (reversibility notes, indexes).

Implementation plan
1) Migration files (after existing `01_init.sql`):
   - `02_identity_event_store.sql`: create `signed_events` table (see BE-01 fields). Add `gen_random_uuid()` default and unique `(account_id, seqno)`.
   - `03_identity_read_models.sql`: create tables `accounts`, `devices`, `device_delegations`, `endorsements`, `endorsement_aggregates`, `recovery_policies`, `recovery_approvals`, `sessions`, `reputation_scores`, plus supporting indexes/constraints from the Phase 1 spec. Use enums as TEXT + CHECK constraints. Add GIN on `endorsements.tags` and JSONB where useful.
   - `04_identity_rate_limits.sql`: create rate-limit/audit tables if used by BE-12.
   - Keep FK references consistent with `uuid` types; mark FKs `ON DELETE CASCADE` for child tables where appropriate.

2) Ensure migrations load in Docker images: update `service/Dockerfile` only if migration path changes; otherwise rely on `MIGRATIONS_DIR` logic already in `db::setup_database`.

3) Local workflows: document in `service/README.md` how to run `sqlx migrate run` and how to reset identity tables for tests.

4) CI/Skaffold: validate migrations inside `skaffold test -p ci`. If new tables require extensions (e.g., pgcrypto), add to init SQL and ensure containers include the extension.

5) Safety: if any migration is irreversible, annotate with a comment. Prefer separate migrations for destructive changes (dropping columns) even if not needed now.

Verification
- `cd service && sqlx migrate run` against local DB.
- `skaffold test -p ci` to confirm migrations apply in the pipeline.
- Manual: inspect `psql -c "\dt"` to verify new tables.

# Adding a Database Migration

## When to use
- Adding/modifying tables, columns, indexes
- NOT for: application code changes without schema impact

## Prerequisites
- Postgres running locally or via `skaffold dev -p dev`
- `sqlx-cli` installed: `cargo install sqlx-cli`

## Steps

1. Find the next available migration number:
   ```bash
   ls service/migrations/*.sql | sort -V | tail -1
   ```
   Use the next integer as your prefix (e.g., if the last is `11_dimension_labels.sql`, use `12_`).

   **Important:** If you are working on a long-lived branch, re-check this number before merging — another PR may have landed a migration with the same number in the meantime.

   Create the file manually:
   ```bash
   touch service/migrations/12_your_description.sql
   ```

2. Write SQL in `migrations/<number>_<descriptive_name>.sql`

3. Run migration locally:
   ```bash
   DATABASE_URL=postgres://postgres:postgres@localhost:5432/tiny-congress sqlx migrate run
   ```

4. Regenerate offline query data:
   ```bash
   cargo sqlx prepare
   ```

5. Commit both the migration and updated `.sqlx/` files

## Verification
- [ ] `just test-backend` passes
- [ ] `just build-backend` passes (sqlx compile-time verification)
- [ ] Migration is idempotent where possible
- [ ] Rollback strategy documented if destructive

## Common failures

| Error | Cause | Fix |
|-------|-------|-----|
| "relation already exists" | Migration ran twice | Check `_sqlx_migrations` table |
| "extension pgmq does not exist" | pgmq not loaded | Ensure init script has `CREATE EXTENSION pgmq;` |
| sqlx compile error | `.sqlx/` out of sync | Re-run `cargo sqlx prepare` |
| "duplicate key value violates unique constraint _sqlx_migrations_pkey" | Two migrations share the same version number | Rename your migration file to the next available number and rerun |

## Prohibited actions
- DO NOT add new tables without explicit approval (see AGENTS.md)
- DO NOT modify existing column types without migration path

## See also
- `service/migrations/` - existing migrations
- `dockerfiles/Dockerfile.postgres` - pgmq extension setup

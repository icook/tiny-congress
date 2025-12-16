# Adding a Database Migration

## When to use
- Adding/modifying tables, columns, indexes
- NOT for: application code changes without schema impact

## Prerequisites
- Postgres running locally or via `skaffold dev -p dev`
- `sqlx-cli` installed: `cargo install sqlx-cli`

## Steps

1. Generate migration file:
   ```bash
   cd service
   sqlx migrate add <descriptive_name>
   ```

2. Write SQL in `migrations/<timestamp>_<descriptive_name>.sql`

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

## Prohibited actions
- DO NOT add new tables without explicit approval (see AGENTS.md)
- DO NOT modify existing column types without migration path

## See also
- `service/migrations/` - existing migrations
- `dockerfiles/Dockerfile.postgres` - pgmq extension setup

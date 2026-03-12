# Database Migrations

## When to use
- Adding/modifying tables, columns, indexes, constraints
- NOT for: application code changes without schema impact

## Prerequisites
- Postgres running locally or via `skaffold dev -p dev`
- `sqlx-cli` installed: `cargo install sqlx-cli`

## Adding a Migration

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

## Change Classification

### Non-breaking (safe to deploy)
- Adding nullable column
- Adding new table
- Adding index
- Relaxing constraint (e.g., NOT NULL -> nullable)

### Breaking (requires coordination)
- Removing column
- Renaming column/table
- Changing column type
- Adding NOT NULL to existing column
- Removing table

## Breaking Change Patterns

### Expand-Contract

1. **Expand**: Add new column alongside old
   ```sql
   ALTER TABLE users ADD COLUMN email_new VARCHAR(255);
   ```

2. **Migrate data**: Backfill in batches
   ```sql
   UPDATE users SET email_new = email WHERE email_new IS NULL LIMIT 1000;
   ```

3. **Switch**: Update application to use new column

4. **Contract**: Remove old column (separate PR, after verification)
   ```sql
   ALTER TABLE users DROP COLUMN email;
   ALTER TABLE users RENAME COLUMN email_new TO email;
   ```

### Feature Flag

1. Add migration with feature flag check in code
2. Deploy with flag off
3. Run migration
4. Enable flag
5. Remove flag and old code path

## Rollback Strategy

Always document rollback in migration file:
```sql
-- Migration: Add status column
ALTER TABLE items ADD COLUMN status VARCHAR(20) DEFAULT 'active';

-- Rollback (manual):
-- ALTER TABLE items DROP COLUMN status;
```

## Testing Migrations

```bash
# Run all tests including migration tests
just test-backend

# Run only migration-specific tests
cd service && cargo test migration

# Run only schema snapshot test
cd service && cargo test schema_snapshot
```

### Updating Schema Snapshot

When you intentionally change the schema via a migration, update the snapshot:

```bash
cd service && cargo insta review
git add service/tests/snapshots/
git commit -m "chore: update schema snapshot after migration"
```

### What Migration Tests Validate

1. **Monotonicity** - Migrations are ordered by version, no duplicates
2. **Idempotency** - Migrations can run multiple times without error
3. **Count Match** - Applied migrations match on-disk migration files
4. **Schema Consistency** - Column types, indexes match expectations
5. **Schema Drift** - Current schema matches committed snapshot
6. **Extensions** - Required extensions (pgcrypto, pgmq) are available

## Verification Checklist

- [ ] `just test-backend` passes
- [ ] `just build-backend` passes (sqlx compile-time verification)
- [ ] Migration is idempotent where possible
- [ ] Rollback SQL documented if destructive
- [ ] No data loss possible
- [ ] Performance impact assessed (large tables)
- [ ] Indexes added for new query patterns

## Common Failures

| Error | Cause | Fix |
|-------|-------|-----|
| "relation already exists" | Migration ran twice | Check `_sqlx_migrations` table |
| "extension pgmq does not exist" | pgmq not loaded | Ensure init script has `CREATE EXTENSION pgmq;` |
| sqlx compile error | `.sqlx/` out of sync | Re-run `cargo sqlx prepare` |
| "duplicate key value violates unique constraint _sqlx_migrations_pkey" | Two migrations share the same version number | Rename your migration file to the next available number and rerun |
| Lock timeout | Long-running migration | Use `CONCURRENTLY` for indexes |
| Data truncation | Column size reduced | Migrate data first |
| Foreign key violation | Referenced data missing | Add data or defer constraint |
| "Migrations are not monotonically ordered" | Version before an existing one | Rename migration file to later version |
| "Schema drift detected" | Schema differs from snapshot | If intentional, run `cargo insta review`; if not, investigate |
| "Applied migrations not found on disk" | Migration file deleted after being applied | Restore file or clean `_sqlx_migrations` |

## Prohibited Actions
- DO NOT add new tables without explicit approval (see CLAUDE.md)
- DO NOT modify existing column types without migration path
- DO NOT drop columns without expand-contract pattern
- DO NOT run migrations during peak traffic

## See Also
- `service/migrations/` - existing migrations
- `dockerfiles/Dockerfile.postgres` - pgmq extension setup
- [Backend Test Patterns](./backend-test-patterns.md) - Database testing patterns
- [Test Writing Skill](../skills/test-writing.md) - LLM decision tree for test placement
- `service/tests/migration_tests.rs` - Migration test implementations
- `service/tests/schema_snapshot.rs` - Schema drift detection

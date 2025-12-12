# Database Schema Changes

## When to use
- Modifying table structure
- Adding/removing columns
- Changing constraints or indexes

## Change classification

### Non-breaking (safe to deploy)
- Adding nullable column
- Adding new table
- Adding index
- Relaxing constraint (e.g., NOT NULL → nullable)

### Breaking (requires coordination)
- Removing column
- Renaming column/table
- Changing column type
- Adding NOT NULL to existing column
- Removing table

## Non-breaking change workflow

1. Write migration (see `adding-migration.md`):
   ```sql
   ALTER TABLE users ADD COLUMN preferences JSONB;
   ```

2. Deploy migration first, then deploy code that uses it

3. No rollback needed - old code ignores new column

## Breaking change workflow

### Option A: Expand-Contract pattern

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

### Option B: Feature flag

1. Add migration with feature flag check in code
2. Deploy with flag off
3. Run migration
4. Enable flag
5. Remove flag and old code path

## Rollback strategy

Always document rollback in migration file:
```sql
-- Migration: Add status column
ALTER TABLE items ADD COLUMN status VARCHAR(20) DEFAULT 'active';

-- Rollback (manual):
-- ALTER TABLE items DROP COLUMN status;
```

## Verification
- [ ] Migration tested locally
- [ ] Rollback SQL documented
- [ ] No data loss possible
- [ ] Performance impact assessed (large tables)
- [ ] Indexes added for new query patterns

## Common failures

| Error | Cause | Fix |
|-------|-------|-----|
| Lock timeout | Long-running migration | Use `CONCURRENTLY` for indexes |
| Data truncation | Column size reduced | Migrate data first |
| Foreign key violation | Referenced data missing | Add data or defer constraint |

## Prohibited actions
- DO NOT add new tables without explicit approval
- DO NOT drop columns without expand-contract pattern
- DO NOT run migrations during peak traffic

## See also
- `docs/playbooks/adding-migration.md`
- `service/migrations/` - existing migrations

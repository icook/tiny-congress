# Migration Testing Playbook

## Overview

This playbook covers migration-specific testing. For general database test patterns
(when to use `test_transaction()` vs `isolated_db()`), see the documentation in
`service/tests/common/mod.rs`.

## Running Migration Tests

```bash
# Run all tests including migration tests
just test-backend

# Run only migration-specific tests
cd service && cargo test migration

# Run only schema snapshot test
cd service && cargo test schema_snapshot
```

## Updating Schema Snapshot

When you intentionally change the schema via a migration, update the snapshot:

```bash
cd service && cargo insta review
git add service/tests/snapshots/
git commit -m "chore: update schema snapshot after migration"
```

## What Migration Tests Validate

1. **Monotonicity** - Migrations are ordered by version, no duplicates
2. **Idempotency** - Migrations can run multiple times without error
3. **Count Match** - Applied migrations match on-disk migration files
4. **Schema Consistency** - Column types, indexes match expectations
5. **Schema Drift** - Current schema matches committed snapshot
6. **Extensions** - Required extensions (pgcrypto, pgmq) are available

## CI Integration

- **All CI runs**: Migration tests run on every PR and master push
- **Speed**: With few migrations, tests are fast (~15-30ms each)
- **Failure**: Blocks merge, requires investigation

## Troubleshooting

### "Migrations are not monotonically ordered"

A migration was added with a timestamp/version before an existing one.

**Fix:** Rename the migration file to have a later timestamp.

### "Schema drift detected"

The schema differs from the committed snapshot.

**Investigation:**
1. Review the diff in the error message
2. If intentional (new migration), regenerate snapshot
3. If unintentional, investigate what changed the schema

### "Applied migrations not found on disk"

A migration file was deleted after being applied.

**Fix:** Either restore the migration file or manually clean `_sqlx_migrations`.

## See Also

- `service/tests/common/mod.rs` - Test infrastructure documentation
- `service/tests/migration_tests.rs` - Migration test implementations
- `service/tests/schema_snapshot.rs` - Schema drift detection
- `docs/playbooks/adding-migration.md` - How to add new migrations

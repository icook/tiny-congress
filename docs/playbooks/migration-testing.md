# Migration Testing Playbook

## Overview

This project uses a two-tier approach to database testing:

| Category | Purpose | Speed | When to Use |
|----------|---------|-------|-------------|
| **Query Tests** | Test SQL logic, CRUD operations | ~1-5ms | Default for 95% of DB tests |
| **Migration Tests** | Test migrations, schema consistency | ~15-30ms | Use `isolated_db()` for full isolation |

## Query Tests (Category A)

Use `test_transaction()` for fast, isolated query tests:

```rust
use common::test_db::test_transaction;
use tc_test_macros::shared_runtime_test;

#[shared_runtime_test]
async fn test_user_query() {
    let mut tx = test_transaction().await;

    // Your test - transaction auto-rolls back on drop
    sqlx::query("INSERT INTO users ...")
        .execute(&mut *tx)
        .await
        .unwrap();
}
```

**Characteristics:**
- Runs in shared testcontainer (one postgres for all tests)
- Uses transaction rollback for isolation
- ~1-5ms overhead per test
- Runs with normal `cargo test`

## Migration Tests (Category B)

Use `isolated_db()` for tests requiring full database isolation:

```rust
use common::test_db::isolated_db;
use tc_test_macros::shared_runtime_test;

#[shared_runtime_test]
async fn test_migration_behavior() {
    let db = isolated_db().await;

    // Full database isolation - can test commits, rollbacks, etc.
    sqlx::query("BEGIN").execute(db.pool()).await.unwrap();
    // ...
}
```

**Characteristics:**
- Creates fresh database via PostgreSQL template copy
- ~15-30ms overhead per test
- Runs with normal `cargo test`

## Running Migration Tests

### Locally

```bash
# Run all tests including migration tests
just test-backend

# Or directly with cargo
cd service && cargo test

# Run only migration-specific tests
cd service && cargo test migration

# Run only schema snapshot test
cd service && cargo test schema_snapshot
```

### Regenerating Schema Snapshot

When you intentionally change the schema via a migration:

```bash
cd service && cargo test -- generate_schema_snapshot --ignored
git add service/tests/snapshots/schema.sql
git commit -m "chore: update schema snapshot after migration XX"
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

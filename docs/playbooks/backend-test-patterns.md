# Backend Testing Patterns

## When to Use

- Writing new Rust tests for the service
- Setting up database integration tests
- Testing GraphQL resolvers
- Troubleshooting test failures

## Test Organization

Tests live in `service/tests/` with `*_tests.rs` naming:

```
service/tests/
├── common/
│   ├── mod.rs           # Test DB utilities (testcontainers, runtime)
│   ├── app_builder.rs   # Test Axum app builder
│   └── graphql.rs       # GraphQL response helpers
├── api_tests.rs         # API endpoint tests
├── build_info_tests.rs  # Unit tests for build info
├── db_tests.rs          # Database integration tests
├── graphql_tests.rs     # GraphQL resolver tests
└── http_tests.rs        # HTTP handler tests
```

## Running Tests

```bash
# All backend tests
just test-backend

# Watch mode (re-runs on changes)
just test-backend-watch

# Single test file
cargo test --test db_tests

# Single test function
cargo test --test db_tests test_crud_operations
```

## Test Types

### 1. Unit Tests (No Database)

Use `#[test]` or `#[tokio::test]` for tests without database dependencies:

```rust
use tinycongress_api::build_info::BuildInfoProvider;

#[test]
fn uses_env_values_when_provided() {
    let provider = BuildInfoProvider::from_lookup(|key| match key {
        "APP_VERSION" => Some("1.2.3".to_string()),
        "GIT_SHA" => Some("abc123".to_string()),
        _ => None,
    });

    let info = provider.build_info();
    assert_eq!(info.version, "1.2.3");
}
```

### 2. Database Integration Tests (Testcontainers)

Use `#[shared_runtime_test]` from `tc_test_macros` for database tests. This avoids "zombie connection" issues by running all tests on a shared Tokio runtime.

#### Primary pattern: `test_transaction()` (95% of tests)

Use for query logic, CRUD operations, and business logic. Fast (~1-5ms setup) with automatic rollback:

```rust
mod common;

use common::test_db::test_transaction;
use sqlx::query_scalar;
use tc_test_macros::shared_runtime_test;

#[shared_runtime_test]
async fn test_db_query() {
    let mut tx = test_transaction().await;

    let result: i32 = query_scalar("SELECT 1")
        .fetch_one(&mut *tx)
        .await
        .expect("Query failed");

    assert_eq!(result, 1);
    // Transaction auto-rolls back on drop
}
```

#### When to use `get_test_db()` directly

Use when you need the pool (e.g., for repository functions that require `&PgPool`):

```rust
use common::test_db::get_test_db;
use tc_test_macros::shared_runtime_test;

#[shared_runtime_test]
async fn test_with_pool() {
    let db = get_test_db().await;
    // Use db.pool() for repository calls
}
```

#### Specialized pattern: `isolated_db()` (full DB isolation)

Use for tests requiring complete database isolation (~15-30ms setup):
- Migration testing (rollback, idempotency)
- Concurrent transaction behavior (SELECT FOR UPDATE, isolation levels)
- Database-level features (LISTEN/NOTIFY, advisory locks)
- Testing explicit BEGIN/COMMIT/ROLLBACK logic

```rust
use common::test_db::isolated_db;
use tc_test_macros::shared_runtime_test;

#[shared_runtime_test]
async fn test_migration_idempotency() {
    let db = isolated_db().await;
    // This database is fully isolated
    // Database is automatically dropped when `db` goes out of scope
}
```

**Why `#[shared_runtime_test]` instead of `#[tokio::test]`?**

`#[tokio::test]` creates a runtime per test. When tests finish, async cleanup may not complete before the runtime is destroyed, leaving "zombie" connections with broken sockets. The shared runtime ensures all async teardown completes properly.

### 3. GraphQL Resolver Tests

Test resolvers without HTTP by executing queries directly on the schema:

```rust
use async_graphql::{EmptySubscription, Schema};
use serde_json::Value;
use tinycongress_api::graphql::{MutationRoot, QueryRoot};

async fn execute_query(query: &str) -> Value {
    let schema = Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(BuildInfoProvider::from_env())
        .finish();
    let response = schema.execute(query).await;
    serde_json::to_value(response).unwrap()
}

#[tokio::test]
async fn test_build_info_query() {
    let query = r#"
        {
            buildInfo {
                version
                gitSha
            }
        }
    "#;

    let result = execute_query(query).await;
    let data = &result["data"]["buildInfo"];

    assert!(data["version"].is_string());
    assert!(!data["version"].as_str().unwrap().is_empty());
}
```

## Testcontainers Setup

### First-Time Setup

Build the custom Postgres image with pgmq extension:

```bash
just build-test-postgres
```

This creates `tc-postgres:local` which tests use by default.

### How It Works

The `common/test_db` module provides:

1. **Shared runtime** - Single Tokio runtime for all tests (via `tc_test_macros`)
2. **Shared container** - One Postgres container reused across tests
3. **Auto migrations** - Migrations run automatically on container start
4. **Template database** - Pre-migrated template for fast `isolated_db()` creation

```rust
// Key test infrastructure components
pub struct TestTransaction { /* auto-rollback on drop */ }
pub struct TestDb { pool, database_url, port }
pub struct IsolatedDb { pool, database_name, database_url }

pub async fn test_transaction() -> TestTransaction;  // 95% of tests
pub async fn get_test_db() -> &'static TestDb;       // When you need the pool
pub async fn isolated_db() -> IsolatedDb;            // Full isolation
```

### CI Configuration

In CI, set `TEST_POSTGRES_IMAGE` to use the pre-built image:

```bash
TEST_POSTGRES_IMAGE=ghcr.io/icook/tiny-congress/postgres:$SHA
```

## Common Patterns

### Testing CRUD Operations

```rust
use common::test_db::test_transaction;
use sqlx::{query, query_scalar};
use tc_test_macros::shared_runtime_test;
use uuid::Uuid;

#[shared_runtime_test]
async fn test_crud_operations() {
    let mut tx = test_transaction().await;

    // Create
    let id = Uuid::new_v4();
    query("INSERT INTO items (id, name) VALUES ($1, $2)")
        .bind(id)
        .bind("Test Item")
        .execute(&mut *tx)
        .await
        .expect("Insert failed");

    // Read
    let count: i64 = query_scalar("SELECT COUNT(*) FROM items WHERE id = $1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .expect("Count failed");
    assert_eq!(count, 1);

    // No cleanup needed - transaction auto-rolls back
}
```

### Testing with Mocked Dependencies

Use constructor injection for testable code:

```rust
// Production code
impl BuildInfoProvider {
    pub fn from_env() -> Self {
        Self::from_lookup(std::env::var)
    }

    pub fn from_lookup<F>(lookup: F) -> Self
    where
        F: Fn(&str) -> Option<String>,
    {
        // Implementation
    }
}

// Test code
#[test]
fn test_with_custom_values() {
    let provider = BuildInfoProvider::from_lookup(|key| match key {
        "APP_VERSION" => Some("test".to_string()),
        _ => None,
    });
    // ...
}
```

### Checking Extension Availability

```rust
use common::test_db::get_test_db;
use sqlx::query_scalar;
use tc_test_macros::shared_runtime_test;

#[shared_runtime_test]
async fn test_pgmq_extension_available() {
    let db = get_test_db().await;

    let exists: bool = query_scalar(
        "SELECT EXISTS (SELECT FROM pg_extension WHERE extname = 'pgmq')"
    )
    .fetch_one(db.pool())
    .await
    .expect("Query failed");

    assert!(exists, "pgmq extension should be available");
}
```

## Troubleshooting

### "Failed to start postgres container"

1. Ensure Docker is running
2. Build the test image: `just build-test-postgres`
3. Check image exists: `docker images | grep tc-postgres`

### "zombie connection" warnings

Use `#[shared_runtime_test]` instead of `#[tokio::test]` for database tests.

### Tests hang or timeout

- Check container logs: `docker logs <container_id>`
- Increase timeout in `common/mod.rs` if needed
- Ensure migrations don't have infinite loops

### "relation does not exist"

Migrations haven't run. The test framework runs them automatically, but check:
- Migration files exist in `service/migrations/`
- No syntax errors in SQL

### Flaky tests

- Don't rely on test ordering
- Use unique IDs (UUID) for test data
- Clean up test data or rely on container isolation

## See Also

- `docs/playbooks/frontend-test-patterns.md` - Frontend testing guide
- `docs/playbooks/adding-migration.md` - Database migration workflow
- `service/tests/common/mod.rs` - Test DB infrastructure (`test_transaction`, `isolated_db`)
- `service/tests/common/app_builder.rs` - Test Axum app builder
- `service/tests/common/graphql.rs` - GraphQL response helpers

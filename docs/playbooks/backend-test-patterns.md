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
use tinycongress_api::build_info::BuildInfo;

#[test]
fn uses_env_values_when_provided() {
    let info = BuildInfo::from_lookup(|key| match key {
        "APP_VERSION" => Some("1.2.3".to_string()),
        "GIT_SHA" => Some("abc123".to_string()),
        _ => None,
    });

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

#### Internal: `get_test_db()` (read-only verification only)

`get_test_db()` provides direct access to the shared pool. **DO NOT use this for tests that write data** - changes persist between tests and cause flaky failures. Use `test_transaction()` or `isolated_db()` instead.

Valid uses:
- Read-only verification (e.g., checking migrations ran, extension exists)
- Internal use by `test_transaction()` and `isolated_db()`

```rust
use common::test_db::get_test_db;
use tc_test_macros::shared_runtime_test;

#[shared_runtime_test]
async fn test_migrations_applied() {
    let db = get_test_db().await;
    // Read-only check - no data written
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (...)")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert!(exists);
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

### 3. Property-Based Tests

Use [proptest](https://docs.rs/proptest) for testing invariants that should hold for all inputs:

**When to use:**
- Pure functions with clear invariants (no side effects, deterministic)
- Roundtrip properties: `decode(encode(x)) == x`
- Idempotency: `f(f(x)) == f(x)`
- Discovering edge cases through random generation

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn roundtrip_encode_decode(bytes: Vec<u8>) {
        let encoded = encode_base64url(&bytes);
        let decoded = decode_base64url(&encoded).unwrap();
        prop_assert_eq!(decoded, bytes);
    }
}
```

Keep strategies tightly scoped to avoid flaky tests:

```rust
// Good: constrained to valid inputs
fn valid_hostname() -> impl Strategy<Value = String> {
    "[a-z]{1,10}\\.[a-z]{2,4}"
}

// Avoid: overly broad strategies
fn any_string() -> impl Strategy<Value = String> {
    ".*"  // May generate inputs that fail for unrelated reasons
}
```

Place property tests in a separate module within the same file:

```rust
#[cfg(test)]
mod tests { /* example-based tests */ }

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;
    // Property tests here
}
```

**Examples:** `crates/tc-crypto/src/lib.rs`

### 4. Table-Driven Boundary Tests

Use table-driven tests for validation with enumerable edge cases:

**When to use:**
- Validation with known boundary conditions
- Specific regression cases (known bug reproductions)
- Test vectors from specifications
- Cases where explicit documentation of boundaries is valuable

```rust
#[test]
fn database_url_scheme_boundaries() {
    let cases = [
        ("postgres://localhost/db", true, "standard postgres"),
        ("postgresql://localhost/db", true, "postgresql alias"),
        ("mysql://localhost/db", false, "wrong scheme"),
        ("", false, "empty URL"),
    ];

    for (url, should_pass, desc) in cases {
        let mut config = Config::default();
        config.database.url = url.into();
        let result = config.validate();
        assert_eq!(result.is_ok(), should_pass, "case '{}': {:?}", desc, result);
    }
}
```

Benefits:
- Explicit documentation of edge cases
- Easy to add new cases
- Clear failure messages

**Examples:** `service/src/config.rs` (validation boundary tests)

### 5. GraphQL Resolver Tests

Test resolvers without HTTP by executing queries directly on the schema:

```rust
use async_graphql::{EmptySubscription, Schema};
use serde_json::Value;
use tinycongress_api::graphql::{MutationRoot, QueryRoot};

async fn execute_query(query: &str) -> Value {
    let schema = Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(BuildInfo::from_env())
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

This creates `tc-postgres:local` which tests use by default. The image is built automatically when running `just test-backend` if it doesn't exist.

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
pub async fn get_test_db() -> &'static TestDb;       // Internal/read-only only
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
impl BuildInfo {
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
    let provider = BuildInfo::from_lookup(|key| match key {
        "APP_VERSION" => Some("test".to_string()),
        _ => None,
    });
    // ...
}
```

### Checking Extension Availability

Read-only verification tests can use `get_test_db()` directly:

```rust
use common::test_db::get_test_db;
use sqlx::query_scalar;
use tc_test_macros::shared_runtime_test;

#[shared_runtime_test]
async fn test_pgmq_extension_available() {
    let db = get_test_db().await;

    // Read-only check - safe to use get_test_db() directly
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

- [Test Writing Skill](../skills/test-writing.md) - LLM decision tree for test placement
- [Frontend Test Patterns](./frontend-test-patterns.md) - Frontend testing guide
- [Adding Migration](./adding-migration.md) - Database migration workflow
- `service/tests/common/mod.rs` - Test DB infrastructure (`test_transaction`, `isolated_db`)
- `service/tests/common/app_builder.rs` - Test Axum app builder
- `service/tests/common/graphql.rs` - GraphQL response helpers

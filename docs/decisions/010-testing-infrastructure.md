# ADR-010: Testing Infrastructure

## Status
Accepted

## Context

TinyCongress's backend tests need real PostgreSQL for SQL query correctness — mocking `sqlx` calls would test the mock, not the query. But spinning up a fresh database per test is too slow (100–300ms for migrations alone), and creating a new Tokio runtime per test (as `#[tokio::test]` does) leaves zombie connections when the runtime drops before async cleanup finishes.

Several tensions shaped this decision:

- **Isolation vs. speed.** Full database isolation (separate DB per test) is safest but slowest. Transaction rollback is fast but prevents testing code that manages its own transactions or uses advisory locks.
- **Shared state vs. test independence.** A shared Postgres container eliminates startup cost but requires careful connection management so tests don't interfere with each other.
- **Ergonomics vs. correctness.** `#[tokio::test]` is convenient but creates per-test runtimes that die before connections are cleaned up. A shared runtime is less obvious but prevents resource leaks.

## Decision

### Single shared Postgres via `OnceCell<TestDb>`

All backend tests share one testcontainers Postgres instance, initialized lazily on first use:

```rust
static TEST_DB: OnceCell<TestDb> = OnceCell::const_new();
```

`TestDb` holds the container handle (`Arc<ContainerAsync<GenericImage>>`), a connection pool (5 max connections, 30s acquire timeout), and port metadata. The container uses a custom `tc-postgres` image matching the production Postgres with pgmq extension.

Initialization flow:
1. Start Postgres container via testcontainers
2. Run all migrations on a single connection
3. Drop the migration connection (required — no active sessions for template creation)
4. Create `tiny_congress_template` database from the migrated `tiny-congress` database
5. Create and return the shared connection pool

### Shared `LazyLock<Runtime>` to prevent zombie connections

```rust
static TEST_RUNTIME: LazyLock<Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create test runtime")
});
```

`#[tokio::test]` creates a new runtime per test. When that runtime drops, async cleanup (connection `ROLLBACK`, pool release) may not complete — leaving zombie connections. The shared runtime outlives all tests, ensuring async teardown always finishes.

Used by `TestTransaction::drop()` (spawns `ROLLBACK` on the shared runtime) and `IsolatedDb::drop()` (spawns connection termination and `DROP DATABASE`).

### `#[shared_runtime_test]` proc-macro

The `test-macros` crate (`crates/test-macros/src/lib.rs`) provides a proc-macro that converts an async test into a synchronous `#[test]` running on the shared runtime:

```rust
// Input:
#[shared_runtime_test]
async fn test_something() {
    let mut tx = test_transaction().await;
    // ...
}

// Expands to:
#[test]
fn test_something() {
    crate::common::test_db::run_test(async {
        let mut tx = test_transaction().await;
        // ...
    })
}
```

The macro validates at compile time: rejects non-async functions, functions with parameters, and generic functions.

### Two isolation tiers

**Tier 1: `test_transaction()` (~1–5ms, 95% of tests)**

Acquires a connection from the shared pool, begins a transaction, and returns a `TestTransaction` that auto-rolls back on drop. Tests see their own writes but the database is never mutated:

```rust
let mut tx = test_transaction().await;
sqlx::query("INSERT INTO accounts ...").execute(&mut *tx).await?;
// tx drops → ROLLBACK; database unchanged
```

Suitable for: CRUD operations, query logic, validation, business rules — anything that doesn't need to manage its own transactions.

**Tier 2: `isolated_db()` (~15–30ms, specialized tests)**

Creates a new database from `tiny_congress_template` via `CREATE DATABASE ... TEMPLATE`. The test gets a dedicated pool with full isolation:

```rust
let db = isolated_db().await;
// Full database — can test transactions, advisory locks, concurrent writes
// Database dropped on scope exit
```

Suitable for: migration tests, concurrent transaction tests, advisory lock tests, code that calls `BEGIN`/`COMMIT` internally.

**Tier 3: `empty_db()` (migration-from-scratch tests)**

Creates a completely empty database with no template. Migrations must be run manually by the test. Used to verify migration idempotency and correctness:

```rust
let db = empty_db().await;
let migrator = load_migrator().await;
migrator.run(db.pool()).await?;
// Truncate tracking table and run again to test idempotency
```

### Template database creation

After running migrations on the main `tiny-congress` database, the initialization creates `tiny_congress_template`:

```sql
CREATE DATABASE "tiny_congress_template" TEMPLATE "tiny-congress"
```

This is a filesystem-level PostgreSQL snapshot — creating a new database from a template is ~15–30ms compared to ~100–300ms to re-run all migrations. PostgreSQL requires zero active connections to the source database for template creation, which is why the migration connection is explicitly closed beforehand.

### Test factories with `AtomicU64` counter

A global atomic counter generates unique identifiers across all tests and threads:

```rust
static FACTORY_COUNTER: AtomicU64 = AtomicU64::new(1);

pub fn next_id() -> u64 {
    FACTORY_COUNTER.fetch_add(1, Ordering::SeqCst)
}
```

Factories use this counter for unique usernames, seeds, and names:

- **`AccountFactory`** — creates accounts with deterministic Ed25519 keypairs derived from a seed byte. Builder pattern: `.with_username("alice")`, `.with_seed(42)`. Different seeds produce different (pubkey, KID) pairs.
- **`TestItemFactory`** — creates test items with unique names. Builder pattern: `.with_name("custom")`.
- **`valid_signup_json()`** — generates a complete signup JSON payload with fresh Ed25519 keypairs, valid backup envelope, and real certificate. Uses `OsRng` for per-call uniqueness.

All factories accept generic `sqlx::Executor`, working with both `TestTransaction` and `IsolatedDb` pools.

### `TestAppBuilder` with presets

`TestAppBuilder` (in `service/tests/common/app_builder.rs`) constructs Axum routers with configurable layers for integration testing:

**Presets:**
- `minimal()` — health check only. For connectivity tests.
- `graphql_only()` — GraphQL routes + health + build info. No database needed.
- `with_mocks()` — full app (GraphQL, REST, identity, health, Swagger, CORS, security headers) backed by `MockIdentityRepo`. Uses real `DefaultIdentityService` so validation matches production.

**Component methods:** `.with_graphql()`, `.with_rest()`, `.with_identity_pool(pool)`, `.with_identity_lazy()`, `.with_swagger()`, `.with_cors(&[...])`, `.with_security_headers_default()`.

Example:
```rust
let db = isolated_db().await;
let app = TestAppBuilder::new()
    .with_identity_pool(db.pool().clone())
    .build();

let response = app.oneshot(Request::builder()
    .method(Method::POST)
    .uri("/auth/signup")
    .body(Body::from(valid_signup_json("alice")))
    .expect("request"))
    .await.expect("response");

assert_eq!(response.status(), StatusCode::CREATED);
```

## Consequences

### Positive
- Most tests run in ~1–5ms (transaction rollback) with zero database setup cost after the first test.
- Zombie connections are eliminated — the shared runtime ensures all async cleanup completes.
- `isolated_db()` provides escape hatch for tests that genuinely need full database isolation without sacrificing speed for the 95% that don't.
- Template-based database creation is ~10x faster than re-running migrations.
- `TestAppBuilder` presets make it easy to write focused integration tests without boilerplate.
- Factories produce valid, unique data across concurrent tests without coordination.

### Negative
- The `OnceCell`/`LazyLock` statics make the test infrastructure harder to understand on first read. The "why" (zombie connections, template prerequisites) is non-obvious.
- Tests sharing a Postgres container cannot run in parallel across different CI jobs — the container is per-process.
- `#[shared_runtime_test]` is a custom proc-macro that developers must learn instead of using the standard `#[tokio::test]`.
- `TestTransaction` auto-rollback means tests cannot verify committed data or test commit/rollback behavior — those require `isolated_db()`.

### Neutral
- The `tc-postgres` custom image must be built locally before first test run. This happens automatically but adds a one-time startup cost.
- `test_transaction()` pool is limited to 5 connections, which is sufficient for single-threaded test execution but would need adjustment for parallel test execution.
- Factory counters reset between test binary invocations, but uniqueness within a single run is guaranteed.

## Alternatives considered

### Separate Postgres per test
- Maximum isolation
- Rejected for speed: ~300ms per test for container startup + migrations is unacceptable at scale
- `isolated_db()` provides per-test isolation when needed, using template cloning instead of full startup

### In-memory SQLite for tests
- Fast, no container needed
- Rejected because PostgreSQL-specific features (advisory locks, `FOR UPDATE`, pgmq, constraint naming) are central to the application. SQLite tests would not catch real bugs.

### `#[tokio::test]` with connection pool cleanup
- Standard approach, no custom macro needed
- Rejected because the per-test runtime drop races with async pool cleanup, causing zombie connections. The shared runtime is the only reliable solution without `block_on` hacks in drop implementations.

### No template database — re-run migrations per `isolated_db()`
- Simpler initialization, no template management
- Rejected for speed: migrations take ~100–300ms vs. ~15–30ms for template cloning. With dozens of isolated DB tests, this adds up.

## References
- [ADR-009: Repo/Service/HTTP Architecture](009-repo-service-http-architecture.md) — the layers this infrastructure tests
- [PR #208: Testing infrastructure](https://github.com/icook/tiny-congress/pull/208) — initial implementation
- [PR #225: Test isolation tiers](https://github.com/icook/tiny-congress/pull/225) — template DB and isolated_db
- `service/tests/common/mod.rs` — TestDb, test_transaction, isolated_db, empty_db
- `service/tests/common/app_builder.rs` — TestAppBuilder
- `service/tests/common/factories/` — AccountFactory, TestItemFactory, valid_signup_json
- `crates/test-macros/src/lib.rs` — #[shared_runtime_test] proc-macro

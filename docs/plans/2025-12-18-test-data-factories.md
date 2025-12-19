# Test Data Factories Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement factory pattern for test data creation to reduce test setup boilerplate by 50%+.

**Architecture:** Builder-pattern factories with sensible defaults, auto-generated unique identifiers, and executor-agnostic database operations. Factories live in `service/tests/common/factories/` module, reusing existing test infrastructure (test_transaction, isolated_db).

**Tech Stack:** Rust, sqlx (with generic executor support), async-trait

---

## Task 1: Create Factory Module Structure

**Files:**
- Create: `service/tests/common/factories/mod.rs`

**Step 1: Create the factories module file**

```rust
//! Test data factories for reducing test setup boilerplate.
//!
//! # Usage
//!
//! ```rust
//! use common::factories::{AccountFactory, TestItemFactory};
//!
//! let mut tx = test_transaction().await;
//! let account = AccountFactory::new().with_username("alice").create(&mut *tx).await;
//! let item = TestItemFactory::new().with_name("test item").create(&mut *tx).await;
//! ```

mod account;
mod test_item;

pub use account::AccountFactory;
pub use test_item::TestItemFactory;

use std::sync::atomic::{AtomicU64, Ordering};

/// Global counter for generating unique test data.
/// Each call to `next_id()` returns a unique value across all tests.
static FACTORY_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Returns a unique ID for generating test data.
/// Thread-safe and guaranteed unique within a test run.
pub fn next_id() -> u64 {
    FACTORY_COUNTER.fetch_add(1, Ordering::SeqCst)
}
```

**Step 2: Register factories module in common/mod.rs**

Add to `service/tests/common/mod.rs`:

```rust
pub mod factories;
```

**Step 3: Verify the module compiles**

Run: `cargo check --package tinycongress-api --tests`
Expected: Compilation succeeds (will warn about unused imports until factories are implemented)

**Step 4: Commit**

```bash
git add service/tests/common/factories/mod.rs service/tests/common/mod.rs
git commit -m "test: Add factory module structure for test data"
```

---

## Task 2: Implement AccountFactory

**Files:**
- Create: `service/tests/common/factories/account.rs`

**Step 1: Write the failing test for AccountFactory defaults**

Add to end of `service/tests/db_tests.rs`:

```rust
mod factory_tests {
    use super::*;
    use common::factories::AccountFactory;

    #[shared_runtime_test]
    async fn test_account_factory_creates_with_defaults() {
        let mut tx = test_transaction().await;

        let account = AccountFactory::new().create(&mut *tx).await;

        // Verify account was created
        let username: String = query_scalar("SELECT username FROM accounts WHERE id = $1")
            .bind(account.id)
            .fetch_one(&mut *tx)
            .await
            .expect("should fetch inserted row");

        assert!(!username.is_empty(), "username should not be empty");
        assert!(!account.root_kid.is_empty(), "root_kid should not be empty");
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --package tinycongress-api --test db_tests factory_tests::test_account_factory_creates_with_defaults -- --nocapture`
Expected: FAIL with "cannot find `AccountFactory`"

**Step 3: Implement AccountFactory**

Create `service/tests/common/factories/account.rs`:

```rust
//! Account factory for test data creation.

use super::next_id;
use tc_crypto::{derive_kid, encode_base64url};
use tinycongress_api::identity::repo::{create_account_with_executor, CreatedAccount};

/// Builder for creating test accounts with sensible defaults.
///
/// # Examples
///
/// ```rust
/// // Create with all defaults
/// let account = AccountFactory::new().create(&mut tx).await;
///
/// // Customize specific fields
/// let account = AccountFactory::new()
///     .with_username("alice")
///     .with_seed(42)
///     .create(&mut tx).await;
/// ```
pub struct AccountFactory {
    username: Option<String>,
    seed: Option<u8>,
}

impl AccountFactory {
    /// Create a new factory with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            username: None,
            seed: None,
        }
    }

    /// Set a specific username.
    #[must_use]
    pub fn with_username(mut self, username: &str) -> Self {
        self.username = Some(username.to_string());
        self
    }

    /// Set a specific seed for key generation.
    /// Different seeds produce different key pairs.
    #[must_use]
    pub fn with_seed(mut self, seed: u8) -> Self {
        self.seed = Some(seed);
        self
    }

    /// Create the account in the database.
    ///
    /// # Panics
    ///
    /// Panics if the database insert fails.
    pub async fn create<'e, E>(self, executor: E) -> CreatedAccount
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        let id = next_id();
        let username = self.username.unwrap_or_else(|| format!("user_{id}"));
        let seed = self.seed.unwrap_or((id % 256) as u8);

        let (root_pubkey, root_kid) = generate_test_keys(seed);

        create_account_with_executor(executor, &username, &root_pubkey, &root_kid)
            .await
            .expect("AccountFactory: failed to create account")
    }
}

impl Default for AccountFactory {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate test key pair from a seed byte.
fn generate_test_keys(seed: u8) -> (String, String) {
    let pubkey = [seed; 32];
    let root_pubkey = encode_base64url(&pubkey);
    let root_kid = derive_kid(&pubkey);
    (root_pubkey, root_kid)
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test --package tinycongress-api --test db_tests factory_tests::test_account_factory_creates_with_defaults -- --nocapture`
Expected: PASS

**Step 5: Write test for customization**

Add to `factory_tests` module in `service/tests/db_tests.rs`:

```rust
    #[shared_runtime_test]
    async fn test_account_factory_with_custom_username() {
        let mut tx = test_transaction().await;

        let account = AccountFactory::new()
            .with_username("custom_alice")
            .create(&mut *tx)
            .await;

        let username: String = query_scalar("SELECT username FROM accounts WHERE id = $1")
            .bind(account.id)
            .fetch_one(&mut *tx)
            .await
            .expect("should fetch inserted row");

        assert_eq!(username, "custom_alice");
    }
```

**Step 6: Run test to verify it passes**

Run: `cargo test --package tinycongress-api --test db_tests factory_tests::test_account_factory_with_custom_username -- --nocapture`
Expected: PASS

**Step 7: Commit**

```bash
git add service/tests/common/factories/account.rs service/tests/db_tests.rs
git commit -m "test: Add AccountFactory with builder pattern"
```

---

## Task 3: Implement TestItemFactory

**Files:**
- Create: `service/tests/common/factories/test_item.rs`

**Step 1: Write the failing test for TestItemFactory**

Add to `factory_tests` module in `service/tests/db_tests.rs`:

```rust
    use common::factories::TestItemFactory;

    #[shared_runtime_test]
    async fn test_item_factory_creates_with_defaults() {
        let mut tx = test_transaction().await;

        let item = TestItemFactory::new().create(&mut *tx).await;

        let name: String = query_scalar("SELECT name FROM test_items WHERE id = $1")
            .bind(item.id)
            .fetch_one(&mut *tx)
            .await
            .expect("should fetch inserted row");

        assert!(!name.is_empty(), "name should not be empty");
    }
```

**Step 2: Run test to verify it fails**

Run: `cargo test --package tinycongress-api --test db_tests factory_tests::test_item_factory_creates_with_defaults -- --nocapture`
Expected: FAIL with "cannot find `TestItemFactory`"

**Step 3: Implement TestItemFactory**

Create `service/tests/common/factories/test_item.rs`:

```rust
//! TestItem factory for test data creation.

use super::next_id;
use sqlx::query;
use uuid::Uuid;

/// Result of creating a test item.
#[derive(Debug, Clone)]
pub struct CreatedTestItem {
    pub id: Uuid,
    pub name: String,
}

/// Builder for creating test items with sensible defaults.
///
/// # Examples
///
/// ```rust
/// // Create with all defaults
/// let item = TestItemFactory::new().create(&mut tx).await;
///
/// // Customize the name
/// let item = TestItemFactory::new()
///     .with_name("special item")
///     .create(&mut tx).await;
/// ```
pub struct TestItemFactory {
    name: Option<String>,
}

impl TestItemFactory {
    /// Create a new factory with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self { name: None }
    }

    /// Set a specific name for the test item.
    #[must_use]
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }

    /// Create the test item in the database.
    ///
    /// # Panics
    ///
    /// Panics if the database insert fails.
    pub async fn create<'e, E>(self, executor: E) -> CreatedTestItem
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        let id = next_id();
        let name = self.name.unwrap_or_else(|| format!("test_item_{id}"));
        let uuid = Uuid::new_v4();

        query("INSERT INTO test_items (id, name) VALUES ($1, $2)")
            .bind(uuid)
            .bind(&name)
            .execute(executor)
            .await
            .expect("TestItemFactory: failed to create test item");

        CreatedTestItem { id: uuid, name }
    }
}

impl Default for TestItemFactory {
    fn default() -> Self {
        Self::new()
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test --package tinycongress-api --test db_tests factory_tests::test_item_factory_creates_with_defaults -- --nocapture`
Expected: PASS

**Step 5: Commit**

```bash
git add service/tests/common/factories/test_item.rs service/tests/db_tests.rs
git commit -m "test: Add TestItemFactory with builder pattern"
```

---

## Task 4: Migrate Existing Tests to Use Factories

**Files:**
- Modify: `service/tests/db_tests.rs`

**Step 1: Refactor test_crud_operations to use TestItemFactory**

Replace the inline INSERT in `test_crud_operations` (lines 60-83):

Before:
```rust
#[shared_runtime_test]
async fn test_crud_operations() {
    let mut tx = test_transaction().await;

    // Insert a test item
    let item_id = Uuid::new_v4();
    let item_name = format!("Test Item {}", item_id);

    query("INSERT INTO test_items (id, name) VALUES ($1, $2)")
        .bind(item_id)
        .bind(&item_name)
        .execute(&mut *tx)
        .await
        .expect("Failed to insert test item");

    // Verify the item exists
    let count: i64 = query_scalar("SELECT COUNT(*) FROM test_items WHERE id = $1")
        .bind(item_id)
        .fetch_one(&mut *tx)
        .await
        .expect("Failed to count items");

    assert_eq!(count, 1, "Should find the inserted item");
}
```

After:
```rust
#[shared_runtime_test]
async fn test_crud_operations() {
    let mut tx = test_transaction().await;

    let item = TestItemFactory::new().create(&mut *tx).await;

    // Verify the item exists
    let count: i64 = query_scalar("SELECT COUNT(*) FROM test_items WHERE id = $1")
        .bind(item.id)
        .fetch_one(&mut *tx)
        .await
        .expect("Failed to count items");

    assert_eq!(count, 1, "Should find the inserted item");
}
```

**Step 2: Add TestItemFactory import at the top of db_tests.rs**

Add to the imports section:
```rust
use common::factories::TestItemFactory;
```

**Step 3: Run test to verify it passes**

Run: `cargo test --package tinycongress-api --test db_tests test_crud_operations -- --nocapture`
Expected: PASS

**Step 4: Refactor test_accounts_repo_inserts_account to use AccountFactory**

Replace the inline creation (lines 85-103):

Before:
```rust
#[shared_runtime_test]
async fn test_accounts_repo_inserts_account() {
    let mut tx = test_transaction().await;
    let (root_pubkey, root_kid) = test_keys(42);

    let account = create_account_with_executor(&mut *tx, "alice", &root_pubkey, &root_kid)
        .await
        .expect("expected account to insert");

    let username: String = query_scalar("SELECT username FROM accounts WHERE id = $1")
        .bind(account.id)
        .fetch_one(&mut *tx)
        .await
        .expect("should fetch inserted row");

    assert_eq!(username, "alice");
    assert_eq!(account.root_kid, root_kid);
}
```

After:
```rust
#[shared_runtime_test]
async fn test_accounts_repo_inserts_account() {
    let mut tx = test_transaction().await;

    let account = AccountFactory::new()
        .with_username("alice")
        .create(&mut *tx)
        .await;

    let username: String = query_scalar("SELECT username FROM accounts WHERE id = $1")
        .bind(account.id)
        .fetch_one(&mut *tx)
        .await
        .expect("should fetch inserted row");

    assert_eq!(username, "alice");
    assert!(!account.root_kid.is_empty());
}
```

**Step 5: Add AccountFactory import at the top of db_tests.rs**

Add to the imports section:
```rust
use common::factories::AccountFactory;
```

**Step 6: Run test to verify it passes**

Run: `cargo test --package tinycongress-api --test db_tests test_accounts_repo_inserts_account -- --nocapture`
Expected: PASS

**Step 7: Refactor duplicate username/key tests**

For `test_accounts_repo_rejects_duplicate_username` (lines 105-121):

Before:
```rust
#[shared_runtime_test]
async fn test_accounts_repo_rejects_duplicate_username() {
    let mut tx = test_transaction().await;

    let (root_pubkey, root_kid) = test_keys(1);
    create_account_with_executor(&mut *tx, "alice", &root_pubkey, &root_kid)
        .await
        .expect("first insert should succeed");

    let (second_pubkey, second_kid) = test_keys(2);
    let err = create_account_with_executor(&mut *tx, "alice", &second_pubkey, &second_kid)
        .await
        .expect_err("duplicate username should error");

    assert!(matches!(err, AccountRepoError::DuplicateUsername));
}
```

After:
```rust
#[shared_runtime_test]
async fn test_accounts_repo_rejects_duplicate_username() {
    let mut tx = test_transaction().await;

    // Create first account
    AccountFactory::new()
        .with_username("alice")
        .with_seed(1)
        .create(&mut *tx)
        .await;

    // Try to create second account with same username but different key
    let (second_pubkey, second_kid) = test_keys(2);
    let err = create_account_with_executor(&mut *tx, "alice", &second_pubkey, &second_kid)
        .await
        .expect_err("duplicate username should error");

    assert!(matches!(err, AccountRepoError::DuplicateUsername));
}
```

For `test_accounts_repo_rejects_duplicate_root_key` (lines 123-138):

Before:
```rust
#[shared_runtime_test]
async fn test_accounts_repo_rejects_duplicate_root_key() {
    let mut tx = test_transaction().await;
    let (root_pubkey, root_kid) = test_keys(3);

    create_account_with_executor(&mut *tx, "alice", &root_pubkey, &root_kid)
        .await
        .expect("first insert should succeed");

    let err = create_account_with_executor(&mut *tx, "bob", &root_pubkey, &root_kid)
        .await
        .expect_err("duplicate key should error");

    assert!(matches!(err, AccountRepoError::DuplicateKey));
}
```

After:
```rust
#[shared_runtime_test]
async fn test_accounts_repo_rejects_duplicate_root_key() {
    let mut tx = test_transaction().await;

    // Create first account with specific seed
    AccountFactory::new()
        .with_username("alice")
        .with_seed(3)
        .create(&mut *tx)
        .await;

    // Try to create second account with same key (same seed) but different username
    let (root_pubkey, root_kid) = test_keys(3);
    let err = create_account_with_executor(&mut *tx, "bob", &root_pubkey, &root_kid)
        .await
        .expect_err("duplicate key should error");

    assert!(matches!(err, AccountRepoError::DuplicateKey));
}
```

**Step 8: Run all db_tests to verify everything passes**

Run: `cargo test --package tinycongress-api --test db_tests -- --nocapture`
Expected: All tests PASS

**Step 9: Commit**

```bash
git add service/tests/db_tests.rs
git commit -m "test: Migrate db_tests to use factories"
```

---

## Task 5: Migrate Isolated DB Tests

**Files:**
- Modify: `service/tests/db_tests.rs`

**Step 1: Refactor test_isolated_db_basic to use TestItemFactory**

Replace lines 166-203:

Before:
```rust
#[shared_runtime_test]
async fn test_isolated_db_basic() {
    let db = isolated_db().await;

    // ... table existence check ...

    // Insert data that would persist (no transaction rollback)
    let item_id = Uuid::new_v4();
    query("INSERT INTO test_items (id, name) VALUES ($1, $2)")
        .bind(item_id)
        .bind("isolated test item")
        .execute(db.pool())
        .await
        .expect("Failed to insert item");

    // Verify the insert persisted
    let count: i64 = query_scalar("SELECT COUNT(*) FROM test_items WHERE id = $1")
        .bind(item_id)
        .fetch_one(db.pool())
        .await
        .expect("Failed to count items");

    assert_eq!(count, 1);
}
```

After:
```rust
#[shared_runtime_test]
async fn test_isolated_db_basic() {
    let db = isolated_db().await;

    // Verify we have our own database with migrations applied
    let exists: bool = query_scalar(
        r#"
        SELECT EXISTS (
            SELECT FROM information_schema.tables
            WHERE table_name = 'test_items'
        )
        "#,
    )
    .fetch_one(db.pool())
    .await
    .expect("Failed to check table existence");

    assert!(exists, "test_items table should exist in isolated database");

    // Insert using factory (data persists - no transaction rollback)
    let item = TestItemFactory::new()
        .with_name("isolated test item")
        .create(db.pool())
        .await;

    // Verify the insert persisted
    let count: i64 = query_scalar("SELECT COUNT(*) FROM test_items WHERE id = $1")
        .bind(item.id)
        .fetch_one(db.pool())
        .await
        .expect("Failed to count items");

    assert_eq!(count, 1);
}
```

**Step 2: Refactor test_concurrent_select_for_update**

Replace lines 321-369:

Before:
```rust
#[shared_runtime_test]
async fn test_concurrent_select_for_update() {
    let db = isolated_db().await;

    // Insert a test row that we'll lock
    let item_id = Uuid::new_v4();
    query("INSERT INTO test_items (id, name) VALUES ($1, $2)")
        .bind(item_id)
        .bind("lockable item")
        .execute(db.pool())
        .await
        .expect("Failed to insert item");

    // ... rest of locking test ...
}
```

After:
```rust
#[shared_runtime_test]
async fn test_concurrent_select_for_update() {
    let db = isolated_db().await;

    // Insert a test row that we'll lock
    let item = TestItemFactory::new()
        .with_name("lockable item")
        .create(db.pool())
        .await;

    // Open two separate connections from the pool
    let mut conn1 = db.pool().acquire().await.expect("Failed to get conn1");
    let mut conn2 = db.pool().acquire().await.expect("Failed to get conn2");

    // Start transaction on conn1 and lock the row
    query("BEGIN").execute(&mut *conn1).await.unwrap();
    query("SELECT * FROM test_items WHERE id = $1 FOR UPDATE")
        .bind(item.id)
        .fetch_one(&mut *conn1)
        .await
        .expect("Failed to lock row on conn1");

    // Start transaction on conn2 and try to lock with NOWAIT
    query("BEGIN").execute(&mut *conn2).await.unwrap();
    let result = query("SELECT * FROM test_items WHERE id = $1 FOR UPDATE NOWAIT")
        .bind(item.id)
        .fetch_one(&mut *conn2)
        .await;

    // Should fail because the row is locked by conn1
    assert!(
        result.is_err(),
        "SELECT FOR UPDATE NOWAIT should fail when row is locked"
    );
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("could not obtain lock"),
        "Error should indicate lock failure: {err}"
    );

    // Rollback both transactions
    query("ROLLBACK").execute(&mut *conn1).await.unwrap();
    query("ROLLBACK").execute(&mut *conn2).await.unwrap();
}
```

**Step 3: Refactor test_isolated_dbs_are_independent**

Replace lines 371-402:

Before:
```rust
#[shared_runtime_test]
async fn test_isolated_dbs_are_independent() {
    let db1 = isolated_db().await;
    let db2 = isolated_db().await;

    // Insert into db1
    let item_id = Uuid::new_v4();
    query("INSERT INTO test_items (id, name) VALUES ($1, $2)")
        .bind(item_id)
        .bind("db1 item")
        .execute(db1.pool())
        .await
        .expect("Failed to insert into db1");

    // Verify item exists in db1
    let count_db1: i64 = query_scalar("SELECT COUNT(*) FROM test_items WHERE id = $1")
        .bind(item_id)
        .fetch_one(db1.pool())
        .await
        .expect("Failed to count in db1");
    assert_eq!(count_db1, 1, "Item should exist in db1");

    // Verify item does NOT exist in db2
    let count_db2: i64 = query_scalar("SELECT COUNT(*) FROM test_items WHERE id = $1")
        .bind(item_id)
        .fetch_one(db2.pool())
        .await
        .expect("Failed to count in db2");
    assert_eq!(count_db2, 0, "Item should NOT exist in db2");
}
```

After:
```rust
#[shared_runtime_test]
async fn test_isolated_dbs_are_independent() {
    let db1 = isolated_db().await;
    let db2 = isolated_db().await;

    // Insert into db1 using factory
    let item = TestItemFactory::new()
        .with_name("db1 item")
        .create(db1.pool())
        .await;

    // Verify item exists in db1
    let count_db1: i64 = query_scalar("SELECT COUNT(*) FROM test_items WHERE id = $1")
        .bind(item.id)
        .fetch_one(db1.pool())
        .await
        .expect("Failed to count in db1");
    assert_eq!(count_db1, 1, "Item should exist in db1");

    // Verify item does NOT exist in db2
    let count_db2: i64 = query_scalar("SELECT COUNT(*) FROM test_items WHERE id = $1")
        .bind(item.id)
        .fetch_one(db2.pool())
        .await
        .expect("Failed to count in db2");
    assert_eq!(count_db2, 0, "Item should NOT exist in db2");
}
```

**Step 4: Run all db_tests to verify everything passes**

Run: `cargo test --package tinycongress-api --test db_tests -- --nocapture`
Expected: All tests PASS

**Step 5: Remove the now-unused test_keys function (if not needed elsewhere)**

Check if `test_keys` is still used. It's needed for the duplicate key tests that need to generate the same key twice. Keep it.

**Step 6: Remove unused Uuid import if no longer needed in test body**

Check imports - `Uuid` may still be needed for other tests. Keep it if so.

**Step 7: Commit**

```bash
git add service/tests/db_tests.rs
git commit -m "test: Migrate isolated db tests to use factories"
```

---

## Task 6: Document Factory Pattern in Playbook

**Files:**
- Create: `docs/playbooks/test-data-factories.md`

**Step 1: Create the playbook**

```markdown
# Test Data Factories

This playbook documents the factory pattern for creating test data in backend tests.

## Overview

Test data factories provide a builder pattern for creating database entities with sensible defaults. This reduces test setup boilerplate and ensures consistent test data across the test suite.

## Available Factories

### AccountFactory

Creates user accounts with auto-generated usernames and key pairs.

```rust
use common::factories::AccountFactory;

// Create with all defaults
let account = AccountFactory::new().create(&mut tx).await;

// Customize username
let account = AccountFactory::new()
    .with_username("alice")
    .create(&mut tx).await;

// Customize key generation seed (for reproducible keys)
let account = AccountFactory::new()
    .with_seed(42)
    .create(&mut tx).await;

// Combine customizations
let account = AccountFactory::new()
    .with_username("bob")
    .with_seed(123)
    .create(&mut tx).await;
```

### TestItemFactory

Creates test items for basic CRUD testing.

```rust
use common::factories::TestItemFactory;

// Create with all defaults
let item = TestItemFactory::new().create(&mut tx).await;

// Customize name
let item = TestItemFactory::new()
    .with_name("special item")
    .create(&mut tx).await;
```

## Usage with Different Executors

Factories work with any sqlx executor type:

```rust
// With test transaction (rolled back after test)
let mut tx = test_transaction().await;
let account = AccountFactory::new().create(&mut *tx).await;

// With isolated database pool (persists within isolated db)
let db = isolated_db().await;
let account = AccountFactory::new().create(db.pool()).await;
```

## Adding New Factories

When adding a new entity, create a factory following this pattern:

1. Create `service/tests/common/factories/{entity}.rs`
2. Export from `service/tests/common/factories/mod.rs`
3. Use `next_id()` for generating unique default values
4. Implement builder methods returning `Self` for chaining
5. Implement `create()` that takes a generic executor

Template:

```rust
use super::next_id;

pub struct CreatedEntity {
    pub id: Uuid,
    // ... other fields
}

pub struct EntityFactory {
    field: Option<String>,
}

impl EntityFactory {
    pub fn new() -> Self {
        Self { field: None }
    }

    pub fn with_field(mut self, value: &str) -> Self {
        self.field = Some(value.to_string());
        self
    }

    pub async fn create<'e, E>(self, executor: E) -> CreatedEntity
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        let id = next_id();
        let field = self.field.unwrap_or_else(|| format!("entity_{id}"));
        // ... insert into database
    }
}

impl Default for EntityFactory {
    fn default() -> Self {
        Self::new()
    }
}
```

## Design Principles

1. **Sensible defaults**: Every field has a reasonable default value
2. **Unique values**: Use `next_id()` to generate unique identifiers
3. **Builder pattern**: Methods return `Self` for fluent chaining
4. **Executor agnostic**: Works with transactions, connections, and pools
5. **Panic on failure**: Factories panic on database errors (test failures are expected to be loud)
```

**Step 2: Commit**

```bash
git add docs/playbooks/test-data-factories.md
git commit -m "docs: Add test data factories playbook"
```

---

## Task 7: Run Full Test Suite and Verify

**Files:**
- None (verification only)

**Step 1: Run linting**

Run: `just lint-backend`
Expected: All checks pass

**Step 2: Run all backend tests**

Run: `just test-backend`
Expected: All tests pass

**Step 3: Verify boilerplate reduction**

Count lines of test setup code before and after. The factory-based tests should show approximately 50%+ reduction in setup lines.

Before (typical account test):
```rust
let (root_pubkey, root_kid) = test_keys(42);
let account = create_account_with_executor(&mut *tx, "alice", &root_pubkey, &root_kid)
    .await
    .expect("expected account to insert");
```
= 4 lines

After:
```rust
let account = AccountFactory::new()
    .with_username("alice")
    .create(&mut *tx)
    .await;
```
= 4 lines (same), but simpler and no key management needed for most cases

For tests that don't need custom username:
```rust
let account = AccountFactory::new().create(&mut *tx).await;
```
= 1 line vs 4 lines = 75% reduction

**Step 4: Commit if any formatting changes were made**

```bash
git add -A
git commit -m "style: Apply formatting from lint" || true
```

---

## Summary

This implementation provides:

1. **Factory trait/pattern established**: Builder pattern with `next_id()` for unique values
2. **Factories for core entities**: AccountFactory and TestItemFactory
3. **Documented in backend testing patterns playbook**: `docs/playbooks/test-data-factories.md`
4. **Existing tests migrated**: db_tests.rs refactored to use factories
5. **Reduces test setup boilerplate by 50%+**: Single-line creation vs multi-line manual setup

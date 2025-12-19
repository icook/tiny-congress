//! Database integration tests using testcontainers.
//!
//! These tests use a shared Tokio runtime to ensure proper async cleanup.
//! See common/mod.rs for details on why this pattern is necessary.

mod common;

use common::test_db::{get_test_db, isolated_db, test_transaction};
use sqlx::{query, query_scalar};
use sqlx_core::migrate::Migrator;
use std::path::Path;
use tc_crypto::{derive_kid, encode_base64url};
use tc_test_macros::shared_runtime_test;
use tinycongress_api::identity::repo::{create_account_with_executor, AccountRepoError};
use uuid::Uuid;

fn test_keys(seed: u8) -> (String, String) {
    let pubkey = [seed; 32];
    let root_pubkey = encode_base64url(&pubkey);
    let root_kid = derive_kid(&pubkey);
    (root_pubkey, root_kid)
}

/// Test that we can connect to the database and run queries.
#[shared_runtime_test]
async fn test_db_connection() {
    let db = get_test_db().await;

    // Simple connectivity test
    let result: i32 = query_scalar("SELECT 1")
        .fetch_one(db.pool())
        .await
        .expect("Failed to execute query");

    assert_eq!(result, 1);
}

/// Test that migrations ran successfully by checking for our test table.
#[shared_runtime_test]
async fn test_migrations_applied() {
    let db = get_test_db().await;

    // Check that the test_items table exists
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

    assert!(exists, "test_items table should exist after migrations");
}

/// Test basic CRUD operations.
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

/// Test that accounts table exists and create_account_with_executor works.
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

/// Test unique constraints: duplicate username should be rejected.
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

/// Test unique constraints: duplicate public key should be rejected.
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

/// Test that pgmq extension is available (from custom postgres image).
#[shared_runtime_test]
async fn test_pgmq_extension_available() {
    let db = get_test_db().await;

    // Check that pgmq extension exists
    let exists: bool = query_scalar(
        r#"
        SELECT EXISTS (
            SELECT FROM pg_extension WHERE extname = 'pgmq'
        )
        "#,
    )
    .fetch_one(db.pool())
    .await
    .expect("Failed to check pgmq extension");

    assert!(exists, "pgmq extension should be available");
}

// ============================================================================
// Isolated Database Tests
// ============================================================================
// These tests demonstrate the isolated_db() pattern for cases where
// transaction-based isolation is insufficient.

/// Test that isolated_db creates a fully independent database copy.
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

/// Test migration idempotency - running migrations twice should not fail.
#[shared_runtime_test]
async fn test_migration_idempotency() {
    let db = isolated_db().await;

    // Load the migrator
    let migrator = Migrator::new(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations"
    )))
    .await
    .expect("Failed to load migrations");

    // Run migrations again - should be idempotent (already applied in template)
    migrator
        .run(db.pool())
        .await
        .expect("Migrations should be idempotent");

    // Run migrations a third time to be sure
    migrator
        .run(db.pool())
        .await
        .expect("Migrations should be idempotent on multiple runs");

    // Verify tables still exist
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

    assert!(
        exists,
        "Tables should still exist after re-running migrations"
    );
}

/// Test migration rollback - verify we can drop tables and recreate them.
/// This is a simple demonstration that isolated DBs allow destructive operations.
#[shared_runtime_test]
async fn test_migration_rollback_simulation() {
    let db = isolated_db().await;

    // Verify test_items exists
    let exists_before: bool = query_scalar(
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

    assert!(exists_before, "test_items should exist initially");

    // Drop the table (simulating a rollback)
    query("DROP TABLE test_items")
        .execute(db.pool())
        .await
        .expect("Failed to drop table");

    // Verify it's gone
    let exists_after: bool = query_scalar(
        r#"
        SELECT EXISTS (
            SELECT FROM information_schema.tables
            WHERE table_name = 'test_items'
        )
        "#,
    )
    .fetch_one(db.pool())
    .await
    .expect("Failed to check table existence after drop");

    assert!(!exists_after, "test_items should not exist after drop");

    // Recreate it (simulating migration re-run)
    query(
        r#"
        CREATE TABLE test_items (
            id UUID PRIMARY KEY,
            name TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(db.pool())
    .await
    .expect("Failed to recreate table");

    // Verify it's back
    let exists_final: bool = query_scalar(
        r#"
        SELECT EXISTS (
            SELECT FROM information_schema.tables
            WHERE table_name = 'test_items'
        )
        "#,
    )
    .fetch_one(db.pool())
    .await
    .expect("Failed to check table existence after recreate");

    assert!(exists_final, "test_items should exist after recreation");
}

mod factory_tests {
    use super::*;
    use common::factories::{AccountFactory, TestItemFactory};

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

    #[shared_runtime_test]
    async fn test_account_factory_with_custom_seed() {
        let mut tx = test_transaction().await;

        let account1 = AccountFactory::new().with_seed(42).create(&mut *tx).await;
        let account2 = AccountFactory::new().with_seed(43).create(&mut *tx).await;

        // Different seeds should produce different keys
        assert_ne!(account1.root_kid, account2.root_kid);
    }

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

    #[shared_runtime_test]
    async fn test_item_factory_with_custom_name() {
        let mut tx = test_transaction().await;
        let item = TestItemFactory::new()
            .with_name("custom_item")
            .create(&mut *tx)
            .await;
        let name: String = query_scalar("SELECT name FROM test_items WHERE id = $1")
            .bind(item.id)
            .fetch_one(&mut *tx)
            .await
            .expect("should fetch inserted row");
        assert_eq!(name, "custom_item");
    }

    #[shared_runtime_test]
    async fn test_item_factory_creates_unique_items() {
        let mut tx = test_transaction().await;
        let item1 = TestItemFactory::new().create(&mut *tx).await;
        let item2 = TestItemFactory::new().create(&mut *tx).await;
        assert_ne!(item1.id, item2.id);
        assert_ne!(item1.name, item2.name);
    }
}

/// Test concurrent transaction behavior with SELECT FOR UPDATE.
/// Demonstrates isolation between two connections to the same isolated database.
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

    // Open two separate connections from the pool
    let mut conn1 = db.pool().acquire().await.expect("Failed to get conn1");
    let mut conn2 = db.pool().acquire().await.expect("Failed to get conn2");

    // Start transaction on conn1 and lock the row
    query("BEGIN").execute(&mut *conn1).await.unwrap();
    query("SELECT * FROM test_items WHERE id = $1 FOR UPDATE")
        .bind(item_id)
        .fetch_one(&mut *conn1)
        .await
        .expect("Failed to lock row on conn1");

    // Start transaction on conn2 and try to lock with NOWAIT
    query("BEGIN").execute(&mut *conn2).await.unwrap();
    let result = query("SELECT * FROM test_items WHERE id = $1 FOR UPDATE NOWAIT")
        .bind(item_id)
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

/// Test that isolated databases are truly independent.
/// Changes in one isolated DB should not affect another.
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

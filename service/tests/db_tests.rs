//! Database integration tests using testcontainers.
//!
//! These tests use a shared Tokio runtime to ensure proper async cleanup.
//! See common/mod.rs for details on why this pattern is necessary.

mod common;

use axum::{
    body::{to_bytes, Body},
    http::{header::CONTENT_TYPE, Method, Request, StatusCode},
};
use common::app_builder::TestAppBuilder;
use common::factories::{AccountFactory, TestItemFactory};
use common::test_db::{get_test_db, isolated_db, test_transaction};
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use sqlx::{query, query_scalar};
use sqlx_core::migrate::Migrator;
use std::path::Path;
use tc_crypto::{derive_kid, encode_base64url};
use tc_test_macros::shared_runtime_test;
use tinycongress_api::identity::repo::{
    create_account_with_executor, create_backup_with_executor, create_device_key_with_executor,
    AccountRepoError, BackupRepoError, DeviceKeyRepoError,
};
use tower::ServiceExt;

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

/// Test that migrations ran successfully by checking for a production table.
#[shared_runtime_test]
async fn test_migrations_applied() {
    let db = get_test_db().await;

    // Check that the accounts table exists (a production table from migrations)
    let exists: bool = query_scalar(
        r#"
        SELECT EXISTS (
            SELECT FROM information_schema.tables
            WHERE table_name = 'accounts'
        )
        "#,
    )
    .fetch_one(db.pool())
    .await
    .expect("Failed to check table existence");

    assert!(exists, "accounts table should exist after migrations");
}

/// Test basic CRUD operations.
#[shared_runtime_test]
async fn test_crud_operations() {
    let mut tx = test_transaction().await;

    let item = TestItemFactory::new()
        .create(&mut *tx)
        .await
        .expect("create item");

    // Verify the item exists
    let count: i64 = query_scalar("SELECT COUNT(*) FROM test_items WHERE id = $1")
        .bind(item.id)
        .fetch_one(&mut *tx)
        .await
        .expect("Failed to count items");

    assert_eq!(count, 1, "Should find the inserted item");
}

/// Test that accounts table exists and create_account_with_executor works.
#[shared_runtime_test]
async fn test_accounts_repo_inserts_account() {
    let mut tx = test_transaction().await;

    let account = AccountFactory::new()
        .with_username("alice")
        .with_seed(42)
        .create(&mut *tx)
        .await
        .expect("create account");

    let username: String = query_scalar("SELECT username FROM accounts WHERE id = $1")
        .bind(account.id)
        .fetch_one(&mut *tx)
        .await
        .expect("should fetch inserted row");

    assert_eq!(username, "alice");

    // Verify the key matches the expected value for seed 42
    let (_, expected_kid) = test_keys(42);
    assert_eq!(account.root_kid, expected_kid);
}

/// Test unique constraints: duplicate username should be rejected.
#[shared_runtime_test]
async fn test_accounts_repo_rejects_duplicate_username() {
    let mut tx = test_transaction().await;

    // Create first account
    AccountFactory::new()
        .with_username("alice")
        .with_seed(1)
        .create(&mut *tx)
        .await
        .expect("create first account");

    // Try to create second account with same username but different key
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

    // Create first account with specific seed
    AccountFactory::new()
        .with_username("alice")
        .with_seed(3)
        .create(&mut *tx)
        .await
        .expect("create first account");

    // Try to create second account with same key (same seed) but different username
    let (root_pubkey, root_kid) = test_keys(3);
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
// Backup Repo Tests
// ============================================================================

/// Build a fake encrypted backup envelope (Argon2id, 90 bytes).
fn fake_backup_envelope() -> Vec<u8> {
    let mut envelope = Vec::with_capacity(90);
    envelope.push(0x01); // version
    envelope.push(0x01); // kdf_id = argon2id
    envelope.extend_from_slice(&[0u8; 12]); // kdf params
    envelope.extend_from_slice(&[0xAA; 16]); // salt
    envelope.extend_from_slice(&[0xBB; 12]); // nonce
    envelope.extend_from_slice(&[0xCC; 48]); // ciphertext
    envelope
}

#[shared_runtime_test]
async fn test_backup_repo_creates_backup() {
    let mut tx = test_transaction().await;

    let account = AccountFactory::new()
        .with_username("backup_user")
        .with_seed(10)
        .create(&mut *tx)
        .await
        .expect("create account");

    let envelope = fake_backup_envelope();
    let salt = &[0xAA; 16];
    let (_, root_kid) = test_keys(10);

    let backup = create_backup_with_executor(
        &mut *tx, account.id, &root_kid, &envelope, salt, "argon2id", 1,
    )
    .await
    .expect("create backup");

    assert_eq!(backup.kid, root_kid);

    let kid_from_db: String = query_scalar("SELECT kid FROM account_backups WHERE account_id = $1")
        .bind(account.id)
        .fetch_one(&mut *tx)
        .await
        .expect("fetch backup");

    assert_eq!(kid_from_db, root_kid);
}

#[shared_runtime_test]
async fn test_backup_repo_rejects_duplicate_account() {
    let mut tx = test_transaction().await;

    let account = AccountFactory::new()
        .with_seed(11)
        .create(&mut *tx)
        .await
        .expect("create account");

    let envelope = fake_backup_envelope();
    let salt = &[0xAA; 16];
    let (_, kid1) = test_keys(11);

    create_backup_with_executor(&mut *tx, account.id, &kid1, &envelope, salt, "argon2id", 1)
        .await
        .expect("create first backup");

    // Second backup for same account should fail (uq_account_backups_account)
    let (_, kid2) = test_keys(12);
    let err =
        create_backup_with_executor(&mut *tx, account.id, &kid2, &envelope, salt, "argon2id", 1)
            .await
            .expect_err("duplicate account backup should fail");

    assert!(matches!(err, BackupRepoError::DuplicateAccount));
}

#[shared_runtime_test]
async fn test_backup_repo_rejects_duplicate_kid() {
    let mut tx = test_transaction().await;

    let account1 = AccountFactory::new()
        .with_seed(13)
        .create(&mut *tx)
        .await
        .expect("create account1");
    let account2 = AccountFactory::new()
        .with_seed(14)
        .create(&mut *tx)
        .await
        .expect("create account2");

    let envelope = fake_backup_envelope();
    let salt = &[0xAA; 16];
    let shared_kid = "shared-kid-value";

    create_backup_with_executor(
        &mut *tx,
        account1.id,
        shared_kid,
        &envelope,
        salt,
        "argon2id",
        1,
    )
    .await
    .expect("create first backup");

    let err = create_backup_with_executor(
        &mut *tx,
        account2.id,
        shared_kid,
        &envelope,
        salt,
        "argon2id",
        1,
    )
    .await
    .expect_err("duplicate kid should fail");

    assert!(matches!(err, BackupRepoError::DuplicateKid));
}

// ============================================================================
// Device Key Repo Tests
// ============================================================================

#[shared_runtime_test]
async fn test_device_key_repo_creates_key() {
    let mut tx = test_transaction().await;

    let account = AccountFactory::new()
        .with_seed(20)
        .create(&mut *tx)
        .await
        .expect("create account");

    let certificate = [0x55u8; 64];
    let device = create_device_key_with_executor(
        &mut *tx,
        account.id,
        "device-kid-1",
        "device-pubkey-b64",
        "My Laptop",
        &certificate,
    )
    .await
    .expect("create device key");

    assert_eq!(device.device_kid, "device-kid-1");

    let name_from_db: String =
        query_scalar("SELECT device_name FROM device_keys WHERE device_kid = $1")
            .bind("device-kid-1")
            .fetch_one(&mut *tx)
            .await
            .expect("fetch device key");

    assert_eq!(name_from_db, "My Laptop");
}

#[shared_runtime_test]
async fn test_device_key_repo_rejects_duplicate_kid() {
    let mut tx = test_transaction().await;

    let account = AccountFactory::new()
        .with_seed(21)
        .create(&mut *tx)
        .await
        .expect("create account");

    let certificate = [0x55u8; 64];
    create_device_key_with_executor(
        &mut *tx,
        account.id,
        "dup-kid",
        "pubkey-1",
        "Device A",
        &certificate,
    )
    .await
    .expect("create first device key");

    let err = create_device_key_with_executor(
        &mut *tx,
        account.id,
        "dup-kid",
        "pubkey-2",
        "Device B",
        &certificate,
    )
    .await
    .expect_err("duplicate kid should fail");

    assert!(matches!(err, DeviceKeyRepoError::DuplicateKid));
}

#[shared_runtime_test]
async fn test_device_key_repo_enforces_max_devices() {
    let mut tx = test_transaction().await;

    let account = AccountFactory::new()
        .with_seed(22)
        .create(&mut *tx)
        .await
        .expect("create account");

    let certificate = [0x55u8; 64];

    // Create 10 device keys (the maximum)
    for i in 0..10 {
        create_device_key_with_executor(
            &mut *tx,
            account.id,
            &format!("kid-{i}"),
            &format!("pubkey-{i}"),
            &format!("Device {i}"),
            &certificate,
        )
        .await
        .unwrap_or_else(|_| panic!("create device key {i}"));
    }

    // 11th should fail
    let err = create_device_key_with_executor(
        &mut *tx,
        account.id,
        "kid-overflow",
        "pubkey-overflow",
        "Device Overflow",
        &certificate,
    )
    .await
    .expect_err("11th device key should fail");

    assert!(matches!(err, DeviceKeyRepoError::MaxDevicesReached));
}

// ============================================================================
// Signup HTTP Handler Integration Tests
// ============================================================================
// These tests exercise the full signup handler with a real Postgres pool,
// covering the transaction-based code path that unit tests cannot reach.

/// Build a valid signup JSON body with real Ed25519 keys and certificate.
fn valid_signup_json(username: &str) -> String {
    let root_signing_key = SigningKey::generate(&mut OsRng);
    let root_pubkey_bytes = root_signing_key.verifying_key().to_bytes();
    let root_pubkey = encode_base64url(&root_pubkey_bytes);

    let device_signing_key = SigningKey::generate(&mut OsRng);
    let device_pubkey_bytes = device_signing_key.verifying_key().to_bytes();
    let device_pubkey = encode_base64url(&device_pubkey_bytes);

    let certificate_sig = root_signing_key.sign(&device_pubkey_bytes);
    let certificate = encode_base64url(&certificate_sig.to_bytes());

    let backup_blob = encode_base64url(&fake_backup_envelope());

    format!(
        r#"{{"username": "{username}", "root_pubkey": "{root_pubkey}", "backup": {{"encrypted_blob": "{backup_blob}"}}, "device": {{"pubkey": "{device_pubkey}", "name": "Test Device", "certificate": "{certificate}"}}}}"#
    )
}

#[shared_runtime_test]
async fn test_signup_handler_success() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(valid_signup_json("signuptest")))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CREATED);

    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(body_str.contains("account_id"));
    assert!(body_str.contains("root_kid"));
    assert!(body_str.contains("device_kid"));
}

#[shared_runtime_test]
async fn test_signup_handler_duplicate_username() {
    let db = isolated_db().await;

    // First signup succeeds
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    let body = valid_signup_json("dupuser");
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);

    // Second signup with same username fails
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    let body = valid_signup_json("dupuser");
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(body_str.contains("Username already taken"));
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

    // Insert using factory (data persists - no transaction rollback)
    let item = TestItemFactory::new()
        .with_name("isolated test item")
        .create(db.pool())
        .await
        .expect("create item");

    // Verify the insert persisted
    let count: i64 = query_scalar("SELECT COUNT(*) FROM test_items WHERE id = $1")
        .bind(item.id)
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

    // Verify production tables still exist
    let exists: bool = query_scalar(
        r#"
        SELECT EXISTS (
            SELECT FROM information_schema.tables
            WHERE table_name = 'accounts'
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

/// Test migration rollback - verify we can drop and recreate tables in isolated DBs.
/// Uses a temporary table to avoid conflicting with other tests' shared state.
#[shared_runtime_test]
async fn test_migration_rollback_simulation() {
    let db = isolated_db().await;

    // Create a temporary table to simulate rollback operations
    query(
        r#"
        CREATE TABLE rollback_test (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            value TEXT NOT NULL
        )
        "#,
    )
    .execute(db.pool())
    .await
    .expect("Failed to create rollback_test table");

    // Verify it exists
    let exists_before: bool = query_scalar(
        r#"
        SELECT EXISTS (
            SELECT FROM information_schema.tables
            WHERE table_name = 'rollback_test'
        )
        "#,
    )
    .fetch_one(db.pool())
    .await
    .expect("Failed to check table existence");

    assert!(exists_before, "rollback_test should exist initially");

    // Drop the table (simulating a rollback)
    query("DROP TABLE rollback_test")
        .execute(db.pool())
        .await
        .expect("Failed to drop table");

    // Verify it's gone
    let exists_after: bool = query_scalar(
        r#"
        SELECT EXISTS (
            SELECT FROM information_schema.tables
            WHERE table_name = 'rollback_test'
        )
        "#,
    )
    .fetch_one(db.pool())
    .await
    .expect("Failed to check table existence after drop");

    assert!(!exists_after, "rollback_test should not exist after drop");

    // Recreate it (simulating migration re-run)
    query(
        r#"
        CREATE TABLE rollback_test (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            value TEXT NOT NULL
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
            WHERE table_name = 'rollback_test'
        )
        "#,
    )
    .fetch_one(db.pool())
    .await
    .expect("Failed to check table existence after recreate");

    assert!(exists_final, "rollback_test should exist after recreation");
}

mod factory_tests {
    use super::*;
    use common::factories::{AccountFactory, TestItemFactory};

    #[shared_runtime_test]
    async fn test_account_factory_creates_with_defaults() {
        let mut tx = test_transaction().await;

        let account = AccountFactory::new()
            .create(&mut *tx)
            .await
            .expect("create account");

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
            .await
            .expect("create account");

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

        let account1 = AccountFactory::new()
            .with_seed(42)
            .create(&mut *tx)
            .await
            .expect("create account 1");
        let account2 = AccountFactory::new()
            .with_seed(43)
            .create(&mut *tx)
            .await
            .expect("create account 2");

        // Different seeds should produce different keys
        assert_ne!(account1.root_kid, account2.root_kid);
    }

    #[shared_runtime_test]
    async fn test_item_factory_creates_with_defaults() {
        let mut tx = test_transaction().await;

        let item = TestItemFactory::new()
            .create(&mut *tx)
            .await
            .expect("create item");

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
            .await
            .expect("create item");
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
        let item1 = TestItemFactory::new()
            .create(&mut *tx)
            .await
            .expect("create item 1");
        let item2 = TestItemFactory::new()
            .create(&mut *tx)
            .await
            .expect("create item 2");
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
    let item = TestItemFactory::new()
        .with_name("lockable item")
        .create(db.pool())
        .await
        .expect("create item");

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

/// Test that isolated databases are truly independent.
/// Changes in one isolated DB should not affect another.
#[shared_runtime_test]
async fn test_isolated_dbs_are_independent() {
    let db1 = isolated_db().await;
    let db2 = isolated_db().await;

    // Insert into db1 using factory
    let item = TestItemFactory::new()
        .with_name("db1 item")
        .create(db1.pool())
        .await
        .expect("create item");

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

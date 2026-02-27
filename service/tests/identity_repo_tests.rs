//! Identity repo integration tests -- account, backup, and device key repositories.

mod common;

use common::factories::{generate_test_keys, AccountFactory};
use common::test_db::{isolated_db, test_transaction};
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use sqlx::query_scalar;
use tc_crypto::{encode_base64url, BackupEnvelope, Kid};
use tc_test_macros::shared_runtime_test;
use tinycongress_api::identity::repo::{
    create_account_with_executor, create_backup_with_executor, create_device_key_with_executor,
    AccountRepoError, BackupRepoError, CreateSignupError, DeviceKeyRepoError, IdentityRepo,
    PgIdentityRepo, ValidatedSignup,
};

/// Build a [`ValidatedSignup`] with real Ed25519 keys and a valid certificate.
///
/// Generates fresh keypairs on each call so concurrent tests don't collide.
fn validated_signup_for_test(username: &str) -> ValidatedSignup {
    let root_signing_key = SigningKey::generate(&mut OsRng);
    let root_pubkey_bytes = root_signing_key.verifying_key().to_bytes();

    let device_signing_key = SigningKey::generate(&mut OsRng);
    let device_pubkey_bytes = device_signing_key.verifying_key().to_bytes();

    let certificate_sig = root_signing_key.sign(&device_pubkey_bytes);

    let envelope = BackupEnvelope::build(
        [0xAA; 16], // salt
        65536,
        3,
        1,           // m_cost, t_cost, p_cost
        [0xBB; 12],  // nonce
        &[0xCC; 48], // ciphertext
    )
    .expect("test envelope");

    ValidatedSignup::new(
        username.to_string(),
        encode_base64url(&root_pubkey_bytes),
        Kid::derive(&root_pubkey_bytes),
        envelope.as_bytes().to_vec(),
        envelope.salt().to_vec(),
        envelope.version(),
        encode_base64url(&device_pubkey_bytes),
        Kid::derive(&device_pubkey_bytes),
        "Test Device".to_string(),
        certificate_sig.to_bytes().to_vec(),
    )
}

fn test_envelope() -> BackupEnvelope {
    BackupEnvelope::build(
        [0xAA; 16], // salt
        65536,
        3,
        1,           // m_cost, t_cost, p_cost
        [0xBB; 12],  // nonce
        &[0xCC; 48], // ciphertext
    )
    .expect("test envelope")
}

// ============================================================================
// Account Repo Tests
// ============================================================================

/// Test that accounts table exists and create_account works.
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
    let (_, expected_kid) = generate_test_keys(42);
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
    let (second_pubkey, second_kid) = generate_test_keys(2);
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
    let (root_pubkey, root_kid) = generate_test_keys(3);
    let err = create_account_with_executor(&mut *tx, "bob", &root_pubkey, &root_kid)
        .await
        .expect_err("duplicate key should error");

    assert!(matches!(err, AccountRepoError::DuplicateKey));
}

// ============================================================================
// Backup Repo Tests
// ============================================================================

#[shared_runtime_test]
async fn test_backup_repo_creates_backup() {
    let mut tx = test_transaction().await;

    let account = AccountFactory::new()
        .with_username("backup_user")
        .with_seed(10)
        .create(&mut *tx)
        .await
        .expect("create account");

    let envelope = test_envelope();
    let (_, root_kid) = generate_test_keys(10);

    let backup = create_backup_with_executor(
        &mut *tx,
        account.id,
        &root_kid,
        envelope.as_bytes(),
        envelope.salt(),
        envelope.version(),
    )
    .await
    .expect("create backup");

    assert_eq!(backup.kid, root_kid);

    let kid_from_db: String = query_scalar("SELECT kid FROM account_backups WHERE account_id = $1")
        .bind(account.id)
        .fetch_one(&mut *tx)
        .await
        .expect("fetch backup");

    assert_eq!(kid_from_db, root_kid.as_str());
}

#[shared_runtime_test]
async fn test_backup_repo_rejects_duplicate_account() {
    let mut tx = test_transaction().await;

    let account = AccountFactory::new()
        .with_seed(11)
        .create(&mut *tx)
        .await
        .expect("create account");

    let envelope = test_envelope();
    let (_, kid1) = generate_test_keys(11);

    create_backup_with_executor(
        &mut *tx,
        account.id,
        &kid1,
        envelope.as_bytes(),
        envelope.salt(),
        envelope.version(),
    )
    .await
    .expect("create first backup");

    // Second backup for same account should fail (uq_account_backups_account)
    let envelope2 = test_envelope();
    let (_, kid2) = generate_test_keys(12);
    let err = create_backup_with_executor(
        &mut *tx,
        account.id,
        &kid2,
        envelope2.as_bytes(),
        envelope2.salt(),
        envelope2.version(),
    )
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

    let envelope = test_envelope();
    let shared_kid = Kid::derive(&[99u8; 32]);

    create_backup_with_executor(
        &mut *tx,
        account1.id,
        &shared_kid,
        envelope.as_bytes(),
        envelope.salt(),
        envelope.version(),
    )
    .await
    .expect("create first backup");

    let envelope2 = test_envelope();
    let err = create_backup_with_executor(
        &mut *tx,
        account2.id,
        &shared_kid,
        envelope2.as_bytes(),
        envelope2.salt(),
        envelope2.version(),
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

    let device_kid = Kid::derive(&[20u8; 32]);
    let certificate = [0x55u8; 64];
    let device = create_device_key_with_executor(
        &mut *tx,
        account.id,
        &device_kid,
        "device-pubkey-b64",
        "My Laptop",
        &certificate,
    )
    .await
    .expect("create device key");

    assert_eq!(device.device_kid, device_kid);

    let name_from_db: String =
        query_scalar("SELECT device_name FROM device_keys WHERE device_kid = $1")
            .bind(device_kid.as_str())
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

    let device_kid = Kid::derive(&[21u8; 32]);
    let certificate = [0x55u8; 64];
    create_device_key_with_executor(
        &mut *tx,
        account.id,
        &device_kid,
        "pubkey-1",
        "Device A",
        &certificate,
    )
    .await
    .expect("create first device key");

    let err = create_device_key_with_executor(
        &mut *tx,
        account.id,
        &device_kid,
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
    for i in 0u8..10 {
        let device_kid = Kid::derive(&[100 + i; 32]);
        create_device_key_with_executor(
            &mut *tx,
            account.id,
            &device_kid,
            &format!("pubkey-{i}"),
            &format!("Device {i}"),
            &certificate,
        )
        .await
        .unwrap_or_else(|_| panic!("create device key {i}"));
    }

    // 11th should fail
    let overflow_kid = Kid::derive(&[200u8; 32]);
    let err = create_device_key_with_executor(
        &mut *tx,
        account.id,
        &overflow_kid,
        "pubkey-overflow",
        "Device Overflow",
        &certificate,
    )
    .await
    .expect_err("11th device key should fail");

    assert!(matches!(err, DeviceKeyRepoError::MaxDevicesReached));
}

// ============================================================================
// PgIdentityRepo — compound create_signup tests
// ============================================================================

/// Happy path: `create_signup` inserts account, backup, and device key atomically.
#[shared_runtime_test]
async fn test_create_signup_inserts_all_three_rows() {
    let db = isolated_db().await;
    let repo = PgIdentityRepo::new(db.pool().clone());

    let data = validated_signup_for_test("signupuser");
    let result = repo.create_signup(&data).await.expect("create_signup");

    assert!(!result.account_id.is_nil());

    // Verify all three tables have a row
    let account_count: i64 = query_scalar("SELECT COUNT(*) FROM accounts WHERE id = $1")
        .bind(result.account_id)
        .fetch_one(db.pool())
        .await
        .expect("count accounts");
    assert_eq!(account_count, 1);

    let backup_count: i64 =
        query_scalar("SELECT COUNT(*) FROM account_backups WHERE account_id = $1")
            .bind(result.account_id)
            .fetch_one(db.pool())
            .await
            .expect("count backups");
    assert_eq!(backup_count, 1);

    let device_count: i64 = query_scalar("SELECT COUNT(*) FROM device_keys WHERE account_id = $1")
        .bind(result.account_id)
        .fetch_one(db.pool())
        .await
        .expect("count device keys");
    assert_eq!(device_count, 1);
}

/// Transaction rollback: if account creation fails (duplicate username),
/// no backup or device key rows should be left behind.
#[shared_runtime_test]
async fn test_create_signup_rolls_back_on_duplicate_username() {
    let db = isolated_db().await;
    let repo = PgIdentityRepo::new(db.pool().clone());

    // First signup succeeds
    let data1 = validated_signup_for_test("rollbackuser");
    let first = repo.create_signup(&data1).await.expect("first signup");

    // Second signup with same username fails at account insert
    let data2 = validated_signup_for_test("rollbackuser");
    let err = repo
        .create_signup(&data2)
        .await
        .expect_err("duplicate username should fail");

    assert!(matches!(
        err,
        CreateSignupError::Account(AccountRepoError::DuplicateUsername)
    ));

    // Verify only the first signup's rows exist — no orphaned rows from the second attempt
    let total_backups: i64 = query_scalar("SELECT COUNT(*) FROM account_backups")
        .fetch_one(db.pool())
        .await
        .expect("count all backups");
    assert_eq!(
        total_backups, 1,
        "second signup's backup should have been rolled back"
    );

    let total_devices: i64 = query_scalar("SELECT COUNT(*) FROM device_keys")
        .fetch_one(db.pool())
        .await
        .expect("count all device keys");
    assert_eq!(
        total_devices, 1,
        "second signup's device key should have been rolled back"
    );

    // Verify the surviving rows belong to the first signup
    let surviving_account: i64 = query_scalar("SELECT COUNT(*) FROM accounts WHERE id = $1")
        .bind(first.account_id)
        .fetch_one(db.pool())
        .await
        .expect("count first account");
    assert_eq!(surviving_account, 1);
}

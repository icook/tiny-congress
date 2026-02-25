//! Identity repo integration tests -- account, backup, and device key repositories.

mod common;

use common::factories::AccountFactory;
use common::test_db::test_transaction;
use sqlx::query_scalar;
use tc_crypto::{encode_base64url, BackupEnvelope, Kid};
use tc_test_macros::shared_runtime_test;
use tinycongress_api::identity::repo::{
    create_account_with_executor, create_backup_with_executor, create_device_key_with_executor,
    AccountRepoError, BackupRepoError, DeviceKeyRepoError,
};

fn test_keys(seed: u8) -> (String, Kid) {
    let pubkey = [seed; 32];
    let root_pubkey = encode_base64url(&pubkey);
    let root_kid = Kid::derive(&pubkey);
    (root_pubkey, root_kid)
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
    let (_, root_kid) = test_keys(10);

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
    let (_, kid1) = test_keys(11);

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
    let (_, kid2) = test_keys(12);
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

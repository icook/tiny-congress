//! Integration tests for backup repository.
//!
//! Tests the `BackupRepo` trait implementation against a real database.

mod common;

use common::test_db::{get_test_db, run_test};
use sqlx_core::query::query;
use tinycongress_api::identity::repo::{BackupRepo, BackupRepoError, PgBackupRepo};
use uuid::Uuid;

/// Helper to create an account for testing backups (backups require a valid account_id)
async fn create_test_account(pool: &sqlx::PgPool) -> Uuid {
    let account_id = Uuid::new_v4();
    let kid = format!("test-kid-{}", Uuid::new_v4());

    query(
        r"
        INSERT INTO accounts (id, username, root_pubkey, root_kid, created_at)
        VALUES ($1, $2, $3, $4, now())
        ",
    )
    .bind(account_id)
    .bind(format!("user-{}", account_id))
    .bind("dGVzdC1wdWJrZXk") // base64url encoded test pubkey
    .bind(&kid)
    .execute(pool)
    .await
    .expect("Failed to create test account");

    account_id
}

/// Test creating a backup successfully
#[test]
fn test_create_backup_success() {
    run_test(async {
        let db = get_test_db().await;
        let repo = PgBackupRepo::new(db.pool().clone());

        let account_id = create_test_account(db.pool()).await;
        let kid = format!("backup-kid-{}", Uuid::new_v4());
        let encrypted_backup = vec![1u8; 100];
        let salt = vec![2u8; 16];

        let result = repo
            .create(account_id, &kid, &encrypted_backup, &salt, "argon2id", 1)
            .await;

        assert!(result.is_ok(), "Should create backup successfully");
        let created = result.expect("backup created");
        assert_eq!(created.kid, kid);
    });
}

/// Test that duplicate account backup returns error
#[test]
fn test_create_backup_duplicate_account() {
    run_test(async {
        let db = get_test_db().await;
        let repo = PgBackupRepo::new(db.pool().clone());

        let account_id = create_test_account(db.pool()).await;
        let kid1 = format!("backup-kid-{}", Uuid::new_v4());
        let kid2 = format!("backup-kid-{}", Uuid::new_v4());
        let encrypted_backup = vec![1u8; 100];
        let salt = vec![2u8; 16];

        // First backup should succeed
        repo.create(account_id, &kid1, &encrypted_backup, &salt, "argon2id", 1)
            .await
            .expect("First backup should succeed");

        // Second backup for same account should fail
        let result = repo
            .create(account_id, &kid2, &encrypted_backup, &salt, "argon2id", 1)
            .await;

        assert!(
            matches!(result, Err(BackupRepoError::DuplicateAccount)),
            "Should return DuplicateAccount error"
        );
    });
}

/// Test that duplicate kid returns error
#[test]
fn test_create_backup_duplicate_kid() {
    run_test(async {
        let db = get_test_db().await;
        let repo = PgBackupRepo::new(db.pool().clone());

        let account_id1 = create_test_account(db.pool()).await;
        let account_id2 = create_test_account(db.pool()).await;
        let kid = format!("backup-kid-{}", Uuid::new_v4());
        let encrypted_backup = vec![1u8; 100];
        let salt = vec![2u8; 16];

        // First backup should succeed
        repo.create(account_id1, &kid, &encrypted_backup, &salt, "argon2id", 1)
            .await
            .expect("First backup should succeed");

        // Backup with same kid for different account should fail
        let result = repo
            .create(account_id2, &kid, &encrypted_backup, &salt, "pbkdf2", 1)
            .await;

        assert!(
            matches!(result, Err(BackupRepoError::DuplicateKid)),
            "Should return DuplicateKid error"
        );
    });
}

/// Test that invalid account_id returns error
#[test]
fn test_create_backup_account_not_found() {
    run_test(async {
        let db = get_test_db().await;
        let repo = PgBackupRepo::new(db.pool().clone());

        let fake_account_id = Uuid::new_v4();
        let kid = format!("backup-kid-{}", Uuid::new_v4());
        let encrypted_backup = vec![1u8; 100];
        let salt = vec![2u8; 16];

        let result = repo
            .create(
                fake_account_id,
                &kid,
                &encrypted_backup,
                &salt,
                "argon2id",
                1,
            )
            .await;

        assert!(
            matches!(result, Err(BackupRepoError::AccountNotFound)),
            "Should return AccountNotFound error"
        );
    });
}

/// Test retrieving a backup by kid
#[test]
fn test_get_backup_by_kid() {
    run_test(async {
        let db = get_test_db().await;
        let repo = PgBackupRepo::new(db.pool().clone());

        let account_id = create_test_account(db.pool()).await;
        let kid = format!("backup-kid-{}", Uuid::new_v4());
        let encrypted_backup = vec![1u8; 100];
        let salt = vec![2u8; 16];

        repo.create(account_id, &kid, &encrypted_backup, &salt, "argon2id", 1)
            .await
            .expect("Should create backup");

        let result = repo.get_by_kid(&kid).await;

        assert!(result.is_ok(), "Should retrieve backup");
        let backup = result.expect("backup retrieved");
        assert_eq!(backup.kid, kid);
        assert_eq!(backup.account_id, account_id);
        assert_eq!(backup.encrypted_backup, encrypted_backup);
        assert_eq!(backup.salt, salt);
        assert_eq!(backup.kdf_algorithm, "argon2id");
        assert_eq!(backup.version, 1);
    });
}

/// Test retrieving non-existent backup returns NotFound
#[test]
fn test_get_backup_not_found() {
    run_test(async {
        let db = get_test_db().await;
        let repo = PgBackupRepo::new(db.pool().clone());

        let result = repo.get_by_kid("nonexistent-kid").await;

        assert!(
            matches!(result, Err(BackupRepoError::NotFound)),
            "Should return NotFound error"
        );
    });
}

/// Test deleting a backup by kid
#[test]
fn test_delete_backup_by_kid() {
    run_test(async {
        let db = get_test_db().await;
        let repo = PgBackupRepo::new(db.pool().clone());

        let account_id = create_test_account(db.pool()).await;
        let kid = format!("backup-kid-{}", Uuid::new_v4());
        let encrypted_backup = vec![1u8; 100];
        let salt = vec![2u8; 16];

        repo.create(account_id, &kid, &encrypted_backup, &salt, "argon2id", 1)
            .await
            .expect("Should create backup");

        // Delete should succeed
        let delete_result = repo.delete_by_kid(&kid).await;
        assert!(delete_result.is_ok(), "Should delete backup");

        // Verify it's gone
        let get_result = repo.get_by_kid(&kid).await;
        assert!(
            matches!(get_result, Err(BackupRepoError::NotFound)),
            "Backup should be deleted"
        );
    });
}

/// Test deleting non-existent backup returns NotFound
#[test]
fn test_delete_backup_not_found() {
    run_test(async {
        let db = get_test_db().await;
        let repo = PgBackupRepo::new(db.pool().clone());

        let result = repo.delete_by_kid("nonexistent-kid").await;

        assert!(
            matches!(result, Err(BackupRepoError::NotFound)),
            "Should return NotFound error"
        );
    });
}

/// Test that deleting account cascades to backup
#[test]
fn test_cascade_delete_on_account_removal() {
    run_test(async {
        let db = get_test_db().await;
        let repo = PgBackupRepo::new(db.pool().clone());
        let pool = db.pool();

        let account_id = create_test_account(pool).await;
        let kid = format!("backup-kid-{}", Uuid::new_v4());
        let encrypted_backup = vec![1u8; 100];
        let salt = vec![2u8; 16];

        repo.create(account_id, &kid, &encrypted_backup, &salt, "argon2id", 1)
            .await
            .expect("Should create backup");

        // Delete the account
        query("DELETE FROM accounts WHERE id = $1")
            .bind(account_id)
            .execute(pool)
            .await
            .expect("Should delete account");

        // Backup should be gone due to CASCADE
        let result = repo.get_by_kid(&kid).await;
        assert!(
            matches!(result, Err(BackupRepoError::NotFound)),
            "Backup should be cascade deleted"
        );
    });
}

//! Backup repository for encrypted key storage operations

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// Backup creation/retrieval result
#[derive(Debug, Clone)]
pub struct BackupRecord {
    pub id: Uuid,
    pub account_id: Uuid,
    pub kid: String,
    pub encrypted_backup: Vec<u8>,
    pub salt: Vec<u8>,
    pub kdf_algorithm: String,
    pub version: i32,
    pub created_at: DateTime<Utc>,
}

/// Backup creation result (subset of fields)
#[derive(Debug, Clone)]
pub struct CreatedBackup {
    pub id: Uuid,
    pub kid: String,
    pub created_at: DateTime<Utc>,
}

/// Error types for backup operations
#[derive(Debug, thiserror::Error)]
pub enum BackupRepoError {
    #[error("backup already exists for this account")]
    DuplicateAccount,
    #[error("backup already exists for this key ID")]
    DuplicateKid,
    #[error("backup not found")]
    NotFound,
    #[error("referenced account does not exist")]
    AccountNotFound,
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

/// Repository trait for backup operations
///
/// This trait abstracts database operations to enable unit testing
/// handlers with mock implementations.
#[async_trait]
pub trait BackupRepo: Send + Sync {
    /// Create a new encrypted backup for an account
    ///
    /// # Errors
    ///
    /// Returns `BackupRepoError::DuplicateAccount` if account already has a backup.
    /// Returns `BackupRepoError::DuplicateKid` if kid is already registered.
    /// Returns `BackupRepoError::AccountNotFound` if `account_id` doesn't exist.
    async fn create(
        &self,
        account_id: Uuid,
        kid: &str,
        encrypted_backup: &[u8],
        salt: &[u8],
        kdf_algorithm: &str,
        version: i32,
    ) -> Result<CreatedBackup, BackupRepoError>;

    /// Retrieve a backup by key ID
    ///
    /// # Errors
    ///
    /// Returns `BackupRepoError::NotFound` if no backup exists for this kid.
    async fn get_by_kid(&self, kid: &str) -> Result<BackupRecord, BackupRepoError>;

    /// Delete a backup by key ID
    ///
    /// # Errors
    ///
    /// Returns `BackupRepoError::NotFound` if no backup exists for this kid.
    async fn delete_by_kid(&self, kid: &str) -> Result<(), BackupRepoError>;
}

/// `PostgreSQL` implementation of [`BackupRepo`]
pub struct PgBackupRepo {
    pool: PgPool,
}

impl PgBackupRepo {
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BackupRepo for PgBackupRepo {
    async fn create(
        &self,
        account_id: Uuid,
        kid: &str,
        encrypted_backup: &[u8],
        salt: &[u8],
        kdf_algorithm: &str,
        version: i32,
    ) -> Result<CreatedBackup, BackupRepoError> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        let result = sqlx::query(
            r"
            INSERT INTO account_backups (id, account_id, kid, encrypted_backup, salt, kdf_algorithm, version, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ",
        )
        .bind(id)
        .bind(account_id)
        .bind(kid)
        .bind(encrypted_backup)
        .bind(salt)
        .bind(kdf_algorithm)
        .bind(version)
        .bind(now)
        .execute(&self.pool)
        .await;

        match result {
            Ok(_) => Ok(CreatedBackup {
                id,
                kid: kid.to_string(),
                created_at: now,
            }),
            Err(e) => {
                if let sqlx::Error::Database(db_err) = &e {
                    // Check for constraint violations
                    if let Some(constraint) = db_err.constraint() {
                        match constraint {
                            "uq_account_backups_account" => {
                                return Err(BackupRepoError::DuplicateAccount)
                            }
                            "uq_account_backups_kid" => return Err(BackupRepoError::DuplicateKid),
                            "account_backups_account_id_fkey" => {
                                return Err(BackupRepoError::AccountNotFound)
                            }
                            _ => {}
                        }
                    }
                }
                Err(BackupRepoError::Database(e))
            }
        }
    }

    async fn get_by_kid(&self, kid: &str) -> Result<BackupRecord, BackupRepoError> {
        let result = sqlx::query_as::<
            _,
            (
                Uuid,
                Uuid,
                String,
                Vec<u8>,
                Vec<u8>,
                String,
                i32,
                DateTime<Utc>,
            ),
        >(
            r"
            SELECT id, account_id, kid, encrypted_backup, salt, kdf_algorithm, version, created_at
            FROM account_backups
            WHERE kid = $1
            ",
        )
        .bind(kid)
        .fetch_optional(&self.pool)
        .await?;

        match result {
            Some((
                id,
                account_id,
                kid,
                encrypted_backup,
                salt,
                kdf_algorithm,
                version,
                created_at,
            )) => Ok(BackupRecord {
                id,
                account_id,
                kid,
                encrypted_backup,
                salt,
                kdf_algorithm,
                version,
                created_at,
            }),
            None => Err(BackupRepoError::NotFound),
        }
    }

    async fn delete_by_kid(&self, kid: &str) -> Result<(), BackupRepoError> {
        let result = sqlx::query(
            r"
            DELETE FROM account_backups
            WHERE kid = $1
            ",
        )
        .bind(kid)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(BackupRepoError::NotFound);
        }

        Ok(())
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[allow(clippy::expect_used)]
pub mod mock {
    //! Mock implementation for testing

    use super::{async_trait, BackupRecord, BackupRepo, BackupRepoError, CreatedBackup, Utc, Uuid};
    use std::sync::Mutex;

    /// Mock backup repository for unit tests.
    pub struct MockBackupRepo {
        /// Preset result to return from `create()`.
        pub create_result: Mutex<Option<Result<CreatedBackup, BackupRepoError>>>,
        /// Preset result to return from `get_by_kid()`.
        pub get_result: Mutex<Option<Result<BackupRecord, BackupRepoError>>>,
        /// Preset result to return from `delete_by_kid()`.
        pub delete_result: Mutex<Option<Result<(), BackupRepoError>>>,
    }

    impl MockBackupRepo {
        /// Create a new mock repository.
        #[must_use]
        pub const fn new() -> Self {
            Self {
                create_result: Mutex::new(None),
                get_result: Mutex::new(None),
                delete_result: Mutex::new(None),
            }
        }

        /// Set the result that `create()` will return.
        ///
        /// # Panics
        ///
        /// Panics if the internal mutex is poisoned.
        pub fn set_create_result(&self, result: Result<CreatedBackup, BackupRepoError>) {
            *self.create_result.lock().expect("lock poisoned") = Some(result);
        }

        /// Set the result that `get_by_kid()` will return.
        ///
        /// # Panics
        ///
        /// Panics if the internal mutex is poisoned.
        pub fn set_get_result(&self, result: Result<BackupRecord, BackupRepoError>) {
            *self.get_result.lock().expect("lock poisoned") = Some(result);
        }

        /// Set the result that `delete_by_kid()` will return.
        ///
        /// # Panics
        ///
        /// Panics if the internal mutex is poisoned.
        pub fn set_delete_result(&self, result: Result<(), BackupRepoError>) {
            *self.delete_result.lock().expect("lock poisoned") = Some(result);
        }
    }

    impl Default for MockBackupRepo {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait]
    impl BackupRepo for MockBackupRepo {
        async fn create(
            &self,
            _account_id: Uuid,
            kid: &str,
            _encrypted_backup: &[u8],
            _salt: &[u8],
            _kdf_algorithm: &str,
            _version: i32,
        ) -> Result<CreatedBackup, BackupRepoError> {
            self.create_result
                .lock()
                .expect("lock poisoned")
                .take()
                .unwrap_or_else(|| {
                    Ok(CreatedBackup {
                        id: Uuid::new_v4(),
                        kid: kid.to_string(),
                        created_at: Utc::now(),
                    })
                })
        }

        async fn get_by_kid(&self, kid: &str) -> Result<BackupRecord, BackupRepoError> {
            self.get_result
                .lock()
                .expect("lock poisoned")
                .take()
                .unwrap_or_else(|| {
                    Ok(BackupRecord {
                        id: Uuid::new_v4(),
                        account_id: Uuid::new_v4(),
                        kid: kid.to_string(),
                        encrypted_backup: vec![0u8; 48],
                        salt: vec![0u8; 16],
                        kdf_algorithm: "argon2id".to_string(),
                        version: 1,
                        created_at: Utc::now(),
                    })
                })
        }

        async fn delete_by_kid(&self, _kid: &str) -> Result<(), BackupRepoError> {
            self.delete_result
                .lock()
                .expect("lock poisoned")
                .take()
                .unwrap_or(Ok(()))
        }
    }
}

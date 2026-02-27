//! Backup repository for encrypted root key storage

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use sqlx::Row;
use tc_crypto::Kid;
use uuid::Uuid;

/// Record returned from backup queries
#[derive(Debug, Clone)]
pub struct BackupRecord {
    pub id: Uuid,
    pub account_id: Uuid,
    pub kid: Kid,
    pub encrypted_backup: Vec<u8>,
    pub salt: Vec<u8>,
    pub version: i32,
    pub created_at: DateTime<Utc>,
}

/// Result of creating a backup
#[derive(Debug, Clone)]
pub struct CreatedBackup {
    pub id: Uuid,
    pub kid: Kid,
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
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

/// Repository trait for backup operations
#[async_trait]
pub trait BackupRepo: Send + Sync {
    /// Create a new encrypted backup
    async fn create(
        &self,
        account_id: Uuid,
        kid: &Kid,
        encrypted_backup: &[u8],
        salt: &[u8],
        version: i32,
    ) -> Result<CreatedBackup, BackupRepoError>;

    /// Retrieve a backup by KID (for recovery)
    async fn get_by_kid(&self, kid: &Kid) -> Result<BackupRecord, BackupRepoError>;

    /// Delete a backup by KID
    async fn delete_by_kid(&self, kid: &Kid) -> Result<(), BackupRepoError>;
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
        kid: &Kid,
        encrypted_backup: &[u8],
        salt: &[u8],
        version: i32,
    ) -> Result<CreatedBackup, BackupRepoError> {
        create_backup(&self.pool, account_id, kid, encrypted_backup, salt, version).await
    }

    async fn get_by_kid(&self, kid: &Kid) -> Result<BackupRecord, BackupRepoError> {
        get_backup_by_kid(&self.pool, kid).await
    }

    async fn delete_by_kid(&self, kid: &Kid) -> Result<(), BackupRepoError> {
        delete_backup_by_kid(&self.pool, kid).await
    }
}

async fn create_backup<'e, E>(
    executor: E,
    account_id: Uuid,
    kid: &Kid,
    encrypted_backup: &[u8],
    salt: &[u8],
    version: i32,
) -> Result<CreatedBackup, BackupRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let id = Uuid::new_v4();
    let now = Utc::now();

    let result = sqlx::query(
        r"
        INSERT INTO account_backups (id, account_id, kid, encrypted_backup, salt, version, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ",
    )
    .bind(id)
    .bind(account_id)
    .bind(kid.as_str())
    .bind(encrypted_backup)
    .bind(salt)
    .bind(version)
    .bind(now)
    .execute(executor)
    .await;

    match result {
        Ok(_) => Ok(CreatedBackup {
            id,
            kid: kid.clone(),
            created_at: now,
        }),
        Err(e) => {
            if let sqlx::Error::Database(db_err) = &e {
                if let Some(constraint) = db_err.constraint() {
                    match constraint {
                        "uq_account_backups_account" => {
                            return Err(BackupRepoError::DuplicateAccount)
                        }
                        "uq_account_backups_kid" => return Err(BackupRepoError::DuplicateKid),
                        _ => {}
                    }
                }
            }
            Err(BackupRepoError::Database(e))
        }
    }
}

/// Create a backup using any executor (pool, connection, or transaction).
///
/// # Errors
///
/// Returns `BackupRepoError::DuplicateAccount` if a backup already exists for this account.
/// Returns `BackupRepoError::DuplicateKid` if a backup already exists for this key ID.
pub async fn create_backup_with_executor<'e, E>(
    executor: E,
    account_id: Uuid,
    kid: &Kid,
    encrypted_backup: &[u8],
    salt: &[u8],
    version: i32,
) -> Result<CreatedBackup, BackupRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    create_backup(executor, account_id, kid, encrypted_backup, salt, version).await
}

/// Retrieve a backup by KID (for recovery).
///
/// # Errors
///
/// Returns `BackupRepoError::NotFound` if no backup exists for this KID.
///
/// # Panics
///
/// Panics if a KID stored in the database fails to parse — this indicates data corruption.
#[allow(clippy::expect_used)]
async fn get_backup_by_kid<'e, E>(executor: E, kid: &Kid) -> Result<BackupRecord, BackupRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let row = sqlx::query(
        r"
        SELECT id, account_id, kid, encrypted_backup, salt, version, created_at
        FROM account_backups
        WHERE kid = $1
        ",
    )
    .bind(kid.as_str())
    .fetch_optional(executor)
    .await?
    .ok_or(BackupRepoError::NotFound)?;

    Ok(BackupRecord {
        id: row.get("id"),
        account_id: row.get("account_id"),
        // A malformed KID in the DB is a data corruption bug, not a user error
        kid: row
            .get::<String, _>("kid")
            .parse()
            .expect("invalid KID in database"),
        encrypted_backup: row.get("encrypted_backup"),
        salt: row.get("salt"),
        version: row.get("version"),
        created_at: row.get("created_at"),
    })
}

async fn delete_backup_by_kid<'e, E>(executor: E, kid: &Kid) -> Result<(), BackupRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let result = sqlx::query("DELETE FROM account_backups WHERE kid = $1")
        .bind(kid.as_str())
        .execute(executor)
        .await?;

    if result.rows_affected() == 0 {
        return Err(BackupRepoError::NotFound);
    }

    Ok(())
}

#[cfg(any(test, feature = "test-utils"))]
#[allow(clippy::expect_used)]
pub mod mock {
    //! Mock implementation for testing

    use super::{async_trait, BackupRecord, BackupRepoError, CreatedBackup, Uuid};
    use chrono::Utc;
    use std::sync::Mutex;
    use tc_crypto::Kid;

    pub struct MockBackupRepo {
        pub create_result: Mutex<Option<Result<CreatedBackup, BackupRepoError>>>,
        pub get_result: Mutex<Option<Result<BackupRecord, BackupRepoError>>>,
        pub delete_result: Mutex<Option<Result<(), BackupRepoError>>>,
    }

    impl MockBackupRepo {
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
    impl super::BackupRepo for MockBackupRepo {
        async fn create(
            &self,
            _account_id: Uuid,
            kid: &Kid,
            _encrypted_backup: &[u8],
            _salt: &[u8],
            _version: i32,
        ) -> Result<CreatedBackup, BackupRepoError> {
            self.create_result
                .lock()
                .expect("lock poisoned")
                .take()
                .unwrap_or_else(|| {
                    Ok(CreatedBackup {
                        id: Uuid::new_v4(),
                        kid: kid.clone(),
                        created_at: Utc::now(),
                    })
                })
        }

        async fn get_by_kid(&self, _kid: &Kid) -> Result<BackupRecord, BackupRepoError> {
            self.get_result
                .lock()
                .expect("lock poisoned")
                .take()
                .unwrap_or(Err(BackupRepoError::NotFound))
        }

        async fn delete_by_kid(&self, _kid: &Kid) -> Result<(), BackupRepoError> {
            self.delete_result
                .lock()
                .expect("lock poisoned")
                .take()
                .unwrap_or(Ok(()))
        }
    }
}

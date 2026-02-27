//! Backup repository for encrypted root key storage

use chrono::{DateTime, Utc};
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
pub(crate) async fn get_backup_by_kid<'e, E>(
    executor: E,
    kid: &Kid,
) -> Result<BackupRecord, BackupRepoError>
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

pub(crate) async fn delete_backup_by_kid<'e, E>(
    executor: E,
    kid: &Kid,
) -> Result<(), BackupRepoError>
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

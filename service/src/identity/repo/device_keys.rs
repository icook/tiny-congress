//! Device key repository for delegated signing keys

use axum::{http::StatusCode, response::IntoResponse, Json};
use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::PgPool;
use sqlx::Row;
use tc_crypto::Kid;
use uuid::Uuid;

/// Record returned from device key queries
#[derive(Debug, Clone)]
pub struct DeviceKeyRecord {
    pub id: Uuid,
    pub account_id: Uuid,
    pub device_kid: Kid,
    pub device_pubkey: String,
    pub device_name: String,
    pub certificate: Vec<u8>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Result of creating a device key
#[derive(Debug, Clone)]
pub struct CreatedDeviceKey {
    pub id: Uuid,
    pub device_kid: Kid,
    pub created_at: DateTime<Utc>,
}

/// Error types for device key operations
#[derive(Debug, thiserror::Error)]
pub enum DeviceKeyRepoError {
    #[error("device key ID already registered")]
    DuplicateKid,
    #[error("device key not found")]
    NotFound,
    #[error("device key has been revoked")]
    AlreadyRevoked,
    #[error("maximum device limit reached")]
    MaxDevicesReached,
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

/// Maximum number of devices per account
const MAX_DEVICES_PER_ACCOUNT: i64 = 10;

async fn insert_device_key<'e, E>(
    executor: E,
    account_id: Uuid,
    device_kid: &Kid,
    device_pubkey: &str,
    device_name: &str,
    certificate: &[u8],
) -> Result<CreatedDeviceKey, DeviceKeyRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let id = Uuid::new_v4();
    let now = Utc::now();

    // Check device count before inserting.
    // Note: in the atomic signup path this runs inside a transaction,
    // so the count is consistent with the insert.
    let result = sqlx::query(
        r"
        INSERT INTO device_keys (id, account_id, device_kid, device_pubkey, device_name, certificate, created_at)
        SELECT $1, $2, $3, $4, $5, $6, $7
        WHERE (SELECT COUNT(*) FROM device_keys WHERE account_id = $2 AND revoked_at IS NULL) < $8
        ",
    )
    .bind(id)
    .bind(account_id)
    .bind(device_kid.as_str())
    .bind(device_pubkey)
    .bind(device_name)
    .bind(certificate)
    .bind(now)
    .bind(MAX_DEVICES_PER_ACCOUNT)
    .execute(executor)
    .await;

    match result {
        Ok(r) => {
            if r.rows_affected() == 0 {
                return Err(DeviceKeyRepoError::MaxDevicesReached);
            }
            Ok(CreatedDeviceKey {
                id,
                device_kid: device_kid.clone(),
                created_at: now,
            })
        }
        Err(e) => {
            if let sqlx::Error::Database(db_err) = &e {
                if let Some(constraint) = db_err.constraint() {
                    if constraint == "uq_device_keys_kid" {
                        return Err(DeviceKeyRepoError::DuplicateKid);
                    }
                }
            }
            Err(DeviceKeyRepoError::Database(e))
        }
    }
}

/// Create a device key within an existing connection (typically a transaction).
///
/// Acquires a `FOR UPDATE` lock on the account row to serialize concurrent
/// device additions. This prevents two requests from both reading count < 10
/// and both inserting, which would exceed `MAX_DEVICES_PER_ACCOUNT` under
/// `READ COMMITTED` isolation.
///
/// # Errors
///
/// Returns `DeviceKeyRepoError::DuplicateKid` if the device key ID is already registered.
/// Returns `DeviceKeyRepoError::MaxDevicesReached` if the account has reached the device limit.
pub async fn create_device_key_with_conn(
    conn: &mut sqlx::PgConnection,
    account_id: Uuid,
    device_kid: &Kid,
    device_pubkey: &str,
    device_name: &str,
    certificate: &[u8],
) -> Result<CreatedDeviceKey, DeviceKeyRepoError> {
    // Lock the account row to serialize concurrent device additions.
    // Fail explicitly if the account doesn't exist rather than letting the
    // FK constraint surface as a generic Database error.
    let locked = sqlx::query("SELECT id FROM accounts WHERE id = $1 FOR UPDATE")
        .bind(account_id)
        .fetch_optional(&mut *conn)
        .await?;
    if locked.is_none() {
        return Err(DeviceKeyRepoError::NotFound);
    }

    insert_device_key(
        &mut *conn,
        account_id,
        device_kid,
        device_pubkey,
        device_name,
        certificate,
    )
    .await
}

#[allow(clippy::expect_used, clippy::needless_pass_by_value)]
fn map_device_key_row(row: sqlx::postgres::PgRow) -> DeviceKeyRecord {
    DeviceKeyRecord {
        id: row.get("id"),
        account_id: row.get("account_id"),
        // A malformed KID in the DB is a data corruption bug, not a user error
        device_kid: row
            .get::<String, _>("device_kid")
            .parse()
            .expect("invalid KID in database"),
        device_pubkey: row.get("device_pubkey"),
        device_name: row.get("device_name"),
        certificate: row.get("certificate"),
        last_used_at: row.get("last_used_at"),
        revoked_at: row.get("revoked_at"),
        created_at: row.get("created_at"),
    }
}

/// List all device keys for an account (including revoked).
///
/// # Errors
///
/// Returns `DeviceKeyRepoError::Database` on database failures.
pub async fn list_device_keys_by_account<'e, E>(
    executor: E,
    account_id: Uuid,
) -> Result<Vec<DeviceKeyRecord>, DeviceKeyRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let rows = sqlx::query(
        r"
        SELECT id, account_id, device_kid, device_pubkey, device_name,
               certificate, last_used_at, revoked_at, created_at
        FROM device_keys
        WHERE account_id = $1
        ORDER BY created_at ASC
        ",
    )
    .bind(account_id)
    .fetch_all(executor)
    .await?;

    Ok(rows.into_iter().map(map_device_key_row).collect())
}

/// Get a device key by KID.
///
/// # Errors
///
/// Returns `DeviceKeyRepoError::NotFound` if no device key matches the given KID.
pub async fn get_device_key_by_kid<'e, E>(
    executor: E,
    device_kid: &Kid,
) -> Result<DeviceKeyRecord, DeviceKeyRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let row = sqlx::query(
        r"
        SELECT id, account_id, device_kid, device_pubkey, device_name,
               certificate, last_used_at, revoked_at, created_at
        FROM device_keys
        WHERE device_kid = $1
        ",
    )
    .bind(device_kid.as_str())
    .fetch_optional(executor)
    .await?
    .ok_or(DeviceKeyRepoError::NotFound)?;

    Ok(map_device_key_row(row))
}

/// Check whether a device key exists but is revoked, or doesn't exist at all.
/// Used by mutation functions when `UPDATE ... WHERE revoked_at IS NULL` affects 0 rows.
async fn not_found_or_revoked(pool: &PgPool, device_kid: &Kid) -> DeviceKeyRepoError {
    let exists = sqlx::query(
        "SELECT revoked_at IS NOT NULL AS is_revoked FROM device_keys WHERE device_kid = $1",
    )
    .bind(device_kid.as_str())
    .fetch_optional(pool)
    .await;

    match exists {
        Ok(Some(row)) if row.get::<bool, _>("is_revoked") => DeviceKeyRepoError::AlreadyRevoked,
        Ok(Some(_) | None) => DeviceKeyRepoError::NotFound,
        Err(e) => DeviceKeyRepoError::Database(e),
    }
}

/// Execute a mutation that targets an active (non-revoked) device key and
/// disambiguate the error when no rows are affected.
async fn ensure_active_device_updated(
    pool: &PgPool,
    result: sqlx::postgres::PgQueryResult,
    device_kid: &Kid,
) -> Result<(), DeviceKeyRepoError> {
    if result.rows_affected() == 0 {
        return Err(not_found_or_revoked(pool, device_kid).await);
    }
    Ok(())
}

/// Revoke a device key (sets `revoked_at`).
///
/// # Errors
///
/// Returns `DeviceKeyRepoError::NotFound` if no device key matches the given KID.
/// Returns `DeviceKeyRepoError::AlreadyRevoked` if the device key was already revoked.
pub async fn revoke_device_key(pool: &PgPool, device_kid: &Kid) -> Result<(), DeviceKeyRepoError> {
    let result = sqlx::query(
        "UPDATE device_keys SET revoked_at = now() WHERE device_kid = $1 AND revoked_at IS NULL",
    )
    .bind(device_kid.as_str())
    .execute(pool)
    .await?;

    ensure_active_device_updated(pool, result, device_kid).await
}

/// Rename a device.
///
/// # Errors
///
/// Returns `DeviceKeyRepoError::NotFound` if no device key matches the given KID.
/// Returns `DeviceKeyRepoError::AlreadyRevoked` if the device key has been revoked.
pub async fn rename_device_key(
    pool: &PgPool,
    device_kid: &Kid,
    new_name: &str,
) -> Result<(), DeviceKeyRepoError> {
    let result = sqlx::query(
        "UPDATE device_keys SET device_name = $1 WHERE device_kid = $2 AND revoked_at IS NULL",
    )
    .bind(new_name)
    .bind(device_kid.as_str())
    .execute(pool)
    .await?;

    ensure_active_device_updated(pool, result, device_kid).await
}

/// Update `last_used_at` timestamp.
///
/// # Errors
///
/// Returns `DeviceKeyRepoError::NotFound` if no device key matches the given KID.
/// Returns `DeviceKeyRepoError::AlreadyRevoked` if the device key has been revoked.
pub async fn touch_device_key(pool: &PgPool, device_kid: &Kid) -> Result<(), DeviceKeyRepoError> {
    let result = sqlx::query(
        "UPDATE device_keys SET last_used_at = now() WHERE device_kid = $1 AND revoked_at IS NULL",
    )
    .bind(device_kid.as_str())
    .execute(pool)
    .await?;

    ensure_active_device_updated(pool, result, device_kid).await
}

impl IntoResponse for DeviceKeyRepoError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::DuplicateKid => (
                StatusCode::CONFLICT,
                Json(json!({ "error": "Device key already registered" })),
            )
                .into_response(),
            Self::MaxDevicesReached => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": "Maximum device limit reached" })),
            )
                .into_response(),
            Self::NotFound | Self::AlreadyRevoked => {
                // Unreachable from create path — indicates a programming error
                tracing::error!(
                    "Unexpected NotFound/AlreadyRevoked from device key create during signup"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "Internal server error" })),
                )
                    .into_response()
            }
            Self::Database(db_err) => {
                tracing::error!("Signup failed (device key): {db_err}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "Internal server error" })),
                )
                    .into_response()
            }
        }
    }
}

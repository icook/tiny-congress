//! Device key repository for delegated signing keys

use async_trait::async_trait;
use chrono::{DateTime, Utc};
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
    #[error("maximum device limit reached")]
    MaxDevicesReached,
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

/// Maximum number of devices per account
const MAX_DEVICES_PER_ACCOUNT: i64 = 10;

/// Repository trait for device key operations
#[async_trait]
pub trait DeviceKeyRepo: Send + Sync {
    /// Register a new device key
    async fn create(
        &self,
        account_id: Uuid,
        device_kid: &Kid,
        device_pubkey: &str,
        device_name: &str,
        certificate: &[u8],
    ) -> Result<CreatedDeviceKey, DeviceKeyRepoError>;

    /// List all device keys for an account (including revoked)
    async fn list_by_account(
        &self,
        account_id: Uuid,
    ) -> Result<Vec<DeviceKeyRecord>, DeviceKeyRepoError>;

    /// Get a device key by KID
    async fn get_by_kid(&self, device_kid: &Kid) -> Result<DeviceKeyRecord, DeviceKeyRepoError>;

    /// Revoke a device key (sets `revoked_at`)
    async fn revoke(&self, device_kid: &Kid) -> Result<(), DeviceKeyRepoError>;

    /// Rename a device
    async fn rename(&self, device_kid: &Kid, new_name: &str) -> Result<(), DeviceKeyRepoError>;

    /// Update `last_used_at` timestamp
    async fn touch(&self, device_kid: &Kid) -> Result<(), DeviceKeyRepoError>;
}

/// `PostgreSQL` implementation of [`DeviceKeyRepo`]
pub struct PgDeviceKeyRepo {
    pool: PgPool,
}

impl PgDeviceKeyRepo {
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DeviceKeyRepo for PgDeviceKeyRepo {
    async fn create(
        &self,
        account_id: Uuid,
        device_kid: &Kid,
        device_pubkey: &str,
        device_name: &str,
        certificate: &[u8],
    ) -> Result<CreatedDeviceKey, DeviceKeyRepoError> {
        create_device_key(
            &self.pool,
            account_id,
            device_kid,
            device_pubkey,
            device_name,
            certificate,
        )
        .await
    }

    async fn list_by_account(
        &self,
        account_id: Uuid,
    ) -> Result<Vec<DeviceKeyRecord>, DeviceKeyRepoError> {
        list_device_keys_by_account(&self.pool, account_id).await
    }

    async fn get_by_kid(&self, device_kid: &Kid) -> Result<DeviceKeyRecord, DeviceKeyRepoError> {
        get_device_key_by_kid(&self.pool, device_kid).await
    }

    async fn revoke(&self, device_kid: &Kid) -> Result<(), DeviceKeyRepoError> {
        revoke_device_key(&self.pool, device_kid).await
    }

    async fn rename(&self, device_kid: &Kid, new_name: &str) -> Result<(), DeviceKeyRepoError> {
        rename_device_key(&self.pool, device_kid, new_name).await
    }

    async fn touch(&self, device_kid: &Kid) -> Result<(), DeviceKeyRepoError> {
        touch_device_key(&self.pool, device_kid).await
    }
}

async fn create_device_key<'e, E>(
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

/// Create a device key using any executor (pool, connection, or transaction).
///
/// # Transaction safety
///
/// The device-count check and INSERT are **not** atomic under `READ COMMITTED`
/// isolation. Callers that add devices to an existing account **must** run this
/// inside a transaction (or hold a `FOR UPDATE` lock on the account row) to
/// prevent concurrent requests from exceeding `MAX_DEVICES_PER_ACCOUNT`.
/// The signup handler is safe because it creates a fresh account in a
/// serialised transaction.
///
/// # Errors
///
/// Returns `DeviceKeyRepoError::DuplicateKid` if the device key ID is already registered.
/// Returns `DeviceKeyRepoError::MaxDevicesReached` if the account has reached the device limit.
pub async fn create_device_key_with_executor<'e, E>(
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
    create_device_key(
        executor,
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

async fn list_device_keys_by_account<'e, E>(
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

async fn get_device_key_by_kid<'e, E>(
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

async fn revoke_device_key<'e, E>(executor: E, device_kid: &Kid) -> Result<(), DeviceKeyRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let result = sqlx::query(
        "UPDATE device_keys SET revoked_at = now() WHERE device_kid = $1 AND revoked_at IS NULL",
    )
    .bind(device_kid.as_str())
    .execute(executor)
    .await?;

    if result.rows_affected() == 0 {
        return Err(DeviceKeyRepoError::NotFound);
    }

    Ok(())
}

async fn rename_device_key<'e, E>(
    executor: E,
    device_kid: &Kid,
    new_name: &str,
) -> Result<(), DeviceKeyRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let result = sqlx::query(
        "UPDATE device_keys SET device_name = $1 WHERE device_kid = $2 AND revoked_at IS NULL",
    )
    .bind(new_name)
    .bind(device_kid.as_str())
    .execute(executor)
    .await?;

    if result.rows_affected() == 0 {
        return Err(DeviceKeyRepoError::NotFound);
    }

    Ok(())
}

async fn touch_device_key<'e, E>(executor: E, device_kid: &Kid) -> Result<(), DeviceKeyRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let result = sqlx::query(
        "UPDATE device_keys SET last_used_at = now() WHERE device_kid = $1 AND revoked_at IS NULL",
    )
    .bind(device_kid.as_str())
    .execute(executor)
    .await?;

    if result.rows_affected() == 0 {
        return Err(DeviceKeyRepoError::NotFound);
    }

    Ok(())
}

#[cfg(any(test, feature = "test-utils"))]
#[allow(clippy::expect_used)]
pub mod mock {
    //! Mock implementation for testing

    use super::{async_trait, CreatedDeviceKey, DeviceKeyRecord, DeviceKeyRepoError, Uuid};
    use chrono::Utc;
    use std::sync::Mutex;
    use tc_crypto::Kid;

    pub struct MockDeviceKeyRepo {
        pub create_result: Mutex<Option<Result<CreatedDeviceKey, DeviceKeyRepoError>>>,
        pub list_result: Mutex<Option<Result<Vec<DeviceKeyRecord>, DeviceKeyRepoError>>>,
        pub get_result: Mutex<Option<Result<DeviceKeyRecord, DeviceKeyRepoError>>>,
        pub revoke_result: Mutex<Option<Result<(), DeviceKeyRepoError>>>,
        pub rename_result: Mutex<Option<Result<(), DeviceKeyRepoError>>>,
        pub touch_result: Mutex<Option<Result<(), DeviceKeyRepoError>>>,
    }

    impl MockDeviceKeyRepo {
        #[must_use]
        pub const fn new() -> Self {
            Self {
                create_result: Mutex::new(None),
                list_result: Mutex::new(None),
                get_result: Mutex::new(None),
                revoke_result: Mutex::new(None),
                rename_result: Mutex::new(None),
                touch_result: Mutex::new(None),
            }
        }

        /// Set the result that `create()` will return.
        ///
        /// # Panics
        ///
        /// Panics if the internal mutex is poisoned.
        pub fn set_create_result(&self, result: Result<CreatedDeviceKey, DeviceKeyRepoError>) {
            *self.create_result.lock().expect("lock poisoned") = Some(result);
        }
    }

    impl Default for MockDeviceKeyRepo {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait]
    impl super::DeviceKeyRepo for MockDeviceKeyRepo {
        async fn create(
            &self,
            _account_id: Uuid,
            device_kid: &Kid,
            _device_pubkey: &str,
            _device_name: &str,
            _certificate: &[u8],
        ) -> Result<CreatedDeviceKey, DeviceKeyRepoError> {
            self.create_result
                .lock()
                .expect("lock poisoned")
                .take()
                .unwrap_or_else(|| {
                    Ok(CreatedDeviceKey {
                        id: Uuid::new_v4(),
                        device_kid: device_kid.clone(),
                        created_at: Utc::now(),
                    })
                })
        }

        async fn list_by_account(
            &self,
            _account_id: Uuid,
        ) -> Result<Vec<DeviceKeyRecord>, DeviceKeyRepoError> {
            self.list_result
                .lock()
                .expect("lock poisoned")
                .take()
                .unwrap_or_else(|| Ok(vec![]))
        }

        async fn get_by_kid(
            &self,
            _device_kid: &Kid,
        ) -> Result<DeviceKeyRecord, DeviceKeyRepoError> {
            self.get_result
                .lock()
                .expect("lock poisoned")
                .take()
                .unwrap_or(Err(DeviceKeyRepoError::NotFound))
        }

        async fn revoke(&self, _device_kid: &Kid) -> Result<(), DeviceKeyRepoError> {
            self.revoke_result
                .lock()
                .expect("lock poisoned")
                .take()
                .unwrap_or(Ok(()))
        }

        async fn rename(
            &self,
            _device_kid: &Kid,
            _new_name: &str,
        ) -> Result<(), DeviceKeyRepoError> {
            self.rename_result
                .lock()
                .expect("lock poisoned")
                .take()
                .unwrap_or(Ok(()))
        }

        async fn touch(&self, _device_kid: &Kid) -> Result<(), DeviceKeyRepoError> {
            self.touch_result
                .lock()
                .expect("lock poisoned")
                .take()
                .unwrap_or(Ok(()))
        }
    }
}

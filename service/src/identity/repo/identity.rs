//! Consolidated identity repository trait
//!
//! Provides a single [`IdentityRepo`] that combines all identity persistence
//! operations (accounts, backups, device keys) plus a compound [`create_signup`]
//! that wraps the three inserts in a single transaction.

use async_trait::async_trait;
use sqlx::PgPool;
use tc_crypto::Kid;
use uuid::Uuid;

use super::accounts::{
    create_account_with_executor, get_account_by_id, get_account_by_username, AccountRecord,
    AccountRepoError, CreatedAccount,
};
use super::backups::{
    create_backup_with_executor, delete_backup_by_kid, get_backup_by_kid, BackupRecord,
    BackupRepoError, CreatedBackup,
};
use super::device_keys::{
    create_device_key_with_executor, get_device_key_by_kid, list_device_keys_by_account,
    rename_device_key, revoke_device_key, touch_device_key, CreatedDeviceKey, DeviceKeyRecord,
    DeviceKeyRepoError,
};
use super::nonces::{check_and_record_nonce, cleanup_expired_nonces, NonceRepoError};

/// Validated signup data ready for persistence.
///
/// All fields have been decoded, validated, and verified by the service layer.
/// The repo trusts this data and only handles persistence.
pub struct ValidatedSignup {
    pub(crate) username: String,
    pub(crate) root_pubkey: String,
    pub(crate) root_kid: Kid,
    pub(crate) backup_bytes: Vec<u8>,
    pub(crate) backup_salt: Vec<u8>,
    pub(crate) backup_version: i32,
    pub(crate) device_pubkey: String,
    pub(crate) device_kid: Kid,
    pub(crate) device_name: String,
    pub(crate) certificate: Vec<u8>,
}

#[cfg(any(test, feature = "test-utils"))]
impl ValidatedSignup {
    /// Construct a `ValidatedSignup` from pre-computed fields.
    ///
    /// Only available in test builds. Callers are responsible for providing
    /// valid cryptographic material. See integration tests for a helper that
    /// generates real Ed25519 keys and certificates.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        username: String,
        root_pubkey: String,
        root_kid: Kid,
        backup_bytes: Vec<u8>,
        backup_salt: Vec<u8>,
        backup_version: i32,
        device_pubkey: String,
        device_kid: Kid,
        device_name: String,
        certificate: Vec<u8>,
    ) -> Self {
        Self {
            username,
            root_pubkey,
            root_kid,
            backup_bytes,
            backup_salt,
            backup_version,
            device_pubkey,
            device_kid,
            device_name,
            certificate,
        }
    }
}

/// Successful signup result.
#[derive(Debug)]
pub struct SignupResult {
    pub account_id: Uuid,
    pub root_kid: Kid,
    pub device_kid: Kid,
}

/// Error from atomic signup, tagged by which step failed.
#[derive(Debug, thiserror::Error)]
pub enum CreateSignupError {
    #[error("account error: {0}")]
    Account(AccountRepoError),
    #[error("backup error: {0}")]
    Backup(BackupRepoError),
    #[error("device key error: {0}")]
    DeviceKey(DeviceKeyRepoError),
    #[error("transaction error: {0}")]
    Transaction(sqlx::Error),
}

/// Consolidated repository trait for identity persistence.
///
/// Combines account, backup, and device key operations into a single trait,
/// plus a compound [`IdentityRepo::create_signup`] that wraps the three inserts
/// in one transaction.
#[async_trait]
pub trait IdentityRepo: Send + Sync {
    // Account operations

    async fn create_account(
        &self,
        username: &str,
        root_pubkey: &str,
        root_kid: &Kid,
    ) -> Result<CreatedAccount, AccountRepoError>;

    async fn get_account_by_id(&self, account_id: Uuid) -> Result<AccountRecord, AccountRepoError>;

    async fn get_account_by_username(
        &self,
        username: &str,
    ) -> Result<AccountRecord, AccountRepoError>;

    // Backup operations

    async fn create_backup(
        &self,
        account_id: Uuid,
        kid: &Kid,
        encrypted_backup: &[u8],
        salt: &[u8],
        version: i32,
    ) -> Result<CreatedBackup, BackupRepoError>;

    async fn get_backup_by_kid(&self, kid: &Kid) -> Result<BackupRecord, BackupRepoError>;

    async fn delete_backup_by_kid(&self, kid: &Kid) -> Result<(), BackupRepoError>;

    // Device key operations

    async fn create_device_key(
        &self,
        account_id: Uuid,
        device_kid: &Kid,
        device_pubkey: &str,
        device_name: &str,
        certificate: &[u8],
    ) -> Result<CreatedDeviceKey, DeviceKeyRepoError>;

    async fn list_device_keys_by_account(
        &self,
        account_id: Uuid,
    ) -> Result<Vec<DeviceKeyRecord>, DeviceKeyRepoError>;

    async fn get_device_key_by_kid(
        &self,
        device_kid: &Kid,
    ) -> Result<DeviceKeyRecord, DeviceKeyRepoError>;

    async fn revoke_device_key(&self, device_kid: &Kid) -> Result<(), DeviceKeyRepoError>;

    async fn rename_device_key(
        &self,
        device_kid: &Kid,
        new_name: &str,
    ) -> Result<(), DeviceKeyRepoError>;

    async fn touch_device_key(&self, device_kid: &Kid) -> Result<(), DeviceKeyRepoError>;

    // Nonce operations (replay prevention)

    /// Record a nonce hash. Returns `NonceRepoError::Replay` if already seen.
    async fn check_and_record_nonce(&self, nonce_hash: &[u8]) -> Result<(), NonceRepoError>;

    /// Delete nonces older than `max_age_secs`. Returns count of deleted rows.
    async fn cleanup_expired_nonces(&self, max_age_secs: i64) -> Result<u64, NonceRepoError>;

    // Compound: atomic signup (account + backup + device key in one transaction)

    async fn create_signup(
        &self,
        data: &ValidatedSignup,
    ) -> Result<SignupResult, CreateSignupError>;
}

/// `PostgreSQL` implementation of [`IdentityRepo`].
pub struct PgIdentityRepo {
    pool: PgPool,
}

impl PgIdentityRepo {
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl IdentityRepo for PgIdentityRepo {
    async fn create_account(
        &self,
        username: &str,
        root_pubkey: &str,
        root_kid: &Kid,
    ) -> Result<CreatedAccount, AccountRepoError> {
        create_account_with_executor(&self.pool, username, root_pubkey, root_kid).await
    }

    async fn get_account_by_id(&self, account_id: Uuid) -> Result<AccountRecord, AccountRepoError> {
        get_account_by_id(&self.pool, account_id).await
    }

    async fn get_account_by_username(
        &self,
        username: &str,
    ) -> Result<AccountRecord, AccountRepoError> {
        get_account_by_username(&self.pool, username).await
    }

    async fn create_backup(
        &self,
        account_id: Uuid,
        kid: &Kid,
        encrypted_backup: &[u8],
        salt: &[u8],
        version: i32,
    ) -> Result<CreatedBackup, BackupRepoError> {
        create_backup_with_executor(&self.pool, account_id, kid, encrypted_backup, salt, version)
            .await
    }

    async fn get_backup_by_kid(&self, kid: &Kid) -> Result<BackupRecord, BackupRepoError> {
        get_backup_by_kid(&self.pool, kid).await
    }

    async fn delete_backup_by_kid(&self, kid: &Kid) -> Result<(), BackupRepoError> {
        delete_backup_by_kid(&self.pool, kid).await
    }

    async fn create_device_key(
        &self,
        account_id: Uuid,
        device_kid: &Kid,
        device_pubkey: &str,
        device_name: &str,
        certificate: &[u8],
    ) -> Result<CreatedDeviceKey, DeviceKeyRepoError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(DeviceKeyRepoError::Database)?;
        let result = create_device_key_with_executor(
            &mut tx,
            account_id,
            device_kid,
            device_pubkey,
            device_name,
            certificate,
        )
        .await?;
        tx.commit().await.map_err(DeviceKeyRepoError::Database)?;
        Ok(result)
    }

    async fn list_device_keys_by_account(
        &self,
        account_id: Uuid,
    ) -> Result<Vec<DeviceKeyRecord>, DeviceKeyRepoError> {
        list_device_keys_by_account(&self.pool, account_id).await
    }

    async fn get_device_key_by_kid(
        &self,
        device_kid: &Kid,
    ) -> Result<DeviceKeyRecord, DeviceKeyRepoError> {
        get_device_key_by_kid(&self.pool, device_kid).await
    }

    async fn revoke_device_key(&self, device_kid: &Kid) -> Result<(), DeviceKeyRepoError> {
        revoke_device_key(&self.pool, device_kid).await
    }

    async fn rename_device_key(
        &self,
        device_kid: &Kid,
        new_name: &str,
    ) -> Result<(), DeviceKeyRepoError> {
        rename_device_key(&self.pool, device_kid, new_name).await
    }

    async fn touch_device_key(&self, device_kid: &Kid) -> Result<(), DeviceKeyRepoError> {
        touch_device_key(&self.pool, device_kid).await
    }

    async fn check_and_record_nonce(&self, nonce_hash: &[u8]) -> Result<(), NonceRepoError> {
        check_and_record_nonce(&self.pool, nonce_hash).await
    }

    async fn cleanup_expired_nonces(&self, max_age_secs: i64) -> Result<u64, NonceRepoError> {
        cleanup_expired_nonces(&self.pool, max_age_secs).await
    }

    async fn create_signup(
        &self,
        data: &ValidatedSignup,
    ) -> Result<SignupResult, CreateSignupError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(CreateSignupError::Transaction)?;

        let account = create_account_with_executor(
            &mut *tx,
            &data.username,
            &data.root_pubkey,
            &data.root_kid,
        )
        .await
        .map_err(CreateSignupError::Account)?;

        create_backup_with_executor(
            &mut *tx,
            account.id,
            &data.root_kid,
            &data.backup_bytes,
            &data.backup_salt,
            data.backup_version,
        )
        .await
        .map_err(CreateSignupError::Backup)?;

        let device = create_device_key_with_executor(
            &mut tx,
            account.id,
            &data.device_kid,
            &data.device_pubkey,
            &data.device_name,
            &data.certificate,
        )
        .await
        .map_err(CreateSignupError::DeviceKey)?;

        tx.commit().await.map_err(CreateSignupError::Transaction)?;

        Ok(SignupResult {
            account_id: account.id,
            root_kid: account.root_kid,
            device_kid: device.device_kid,
        })
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[allow(clippy::expect_used)]
pub mod mock {
    //! Mock identity repo for unit testing.
    //!
    //! Only `create_signup` is configurable; individual methods return
    //! generic errors since they are not exercised in service-layer tests.

    use super::{
        async_trait, AccountRecord, AccountRepoError, BackupRecord, BackupRepoError,
        CreateSignupError, CreatedAccount, CreatedBackup, CreatedDeviceKey, DeviceKeyRecord,
        DeviceKeyRepoError, IdentityRepo, Kid, NonceRepoError, SignupResult, Uuid, ValidatedSignup,
    };
    use std::sync::Mutex;

    /// Mock identity repo with a configurable `create_signup` result.
    pub struct MockIdentityRepo {
        pub signup_result: Mutex<Option<Result<SignupResult, CreateSignupError>>>,
    }

    impl MockIdentityRepo {
        #[must_use]
        pub const fn new() -> Self {
            Self {
                signup_result: Mutex::new(None),
            }
        }

        /// Set the result that [`IdentityRepo::create_signup`] will return.
        ///
        /// # Panics
        ///
        /// Panics if the internal mutex is poisoned.
        pub fn set_signup_result(&self, result: Result<SignupResult, CreateSignupError>) {
            *self.signup_result.lock().expect("lock poisoned") = Some(result);
        }
    }

    impl Default for MockIdentityRepo {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait]
    impl IdentityRepo for MockIdentityRepo {
        async fn create_account(
            &self,
            _username: &str,
            _root_pubkey: &str,
            root_kid: &Kid,
        ) -> Result<CreatedAccount, AccountRepoError> {
            Ok(CreatedAccount {
                id: Uuid::new_v4(),
                root_kid: root_kid.clone(),
            })
        }

        async fn get_account_by_id(
            &self,
            _account_id: Uuid,
        ) -> Result<AccountRecord, AccountRepoError> {
            Err(AccountRepoError::NotFound)
        }

        async fn get_account_by_username(
            &self,
            _username: &str,
        ) -> Result<AccountRecord, AccountRepoError> {
            Err(AccountRepoError::NotFound)
        }

        async fn create_backup(
            &self,
            _account_id: Uuid,
            kid: &Kid,
            _encrypted_backup: &[u8],
            _salt: &[u8],
            _version: i32,
        ) -> Result<CreatedBackup, BackupRepoError> {
            Ok(CreatedBackup {
                id: Uuid::new_v4(),
                kid: kid.clone(),
                created_at: chrono::Utc::now(),
            })
        }

        async fn get_backup_by_kid(&self, _kid: &Kid) -> Result<BackupRecord, BackupRepoError> {
            Err(BackupRepoError::NotFound)
        }

        async fn delete_backup_by_kid(&self, _kid: &Kid) -> Result<(), BackupRepoError> {
            Ok(())
        }

        async fn create_device_key(
            &self,
            _account_id: Uuid,
            device_kid: &Kid,
            _device_pubkey: &str,
            _device_name: &str,
            _certificate: &[u8],
        ) -> Result<CreatedDeviceKey, DeviceKeyRepoError> {
            Ok(CreatedDeviceKey {
                id: Uuid::new_v4(),
                device_kid: device_kid.clone(),
                created_at: chrono::Utc::now(),
            })
        }

        async fn list_device_keys_by_account(
            &self,
            _account_id: Uuid,
        ) -> Result<Vec<DeviceKeyRecord>, DeviceKeyRepoError> {
            Ok(vec![])
        }

        async fn get_device_key_by_kid(
            &self,
            _device_kid: &Kid,
        ) -> Result<DeviceKeyRecord, DeviceKeyRepoError> {
            Err(DeviceKeyRepoError::NotFound)
        }

        async fn revoke_device_key(&self, _device_kid: &Kid) -> Result<(), DeviceKeyRepoError> {
            Ok(())
        }

        async fn rename_device_key(
            &self,
            _device_kid: &Kid,
            _new_name: &str,
        ) -> Result<(), DeviceKeyRepoError> {
            Ok(())
        }

        async fn touch_device_key(&self, _device_kid: &Kid) -> Result<(), DeviceKeyRepoError> {
            Ok(())
        }

        async fn check_and_record_nonce(&self, _nonce_hash: &[u8]) -> Result<(), NonceRepoError> {
            Ok(())
        }

        async fn cleanup_expired_nonces(&self, _max_age_secs: i64) -> Result<u64, NonceRepoError> {
            Ok(0)
        }

        async fn create_signup(
            &self,
            _data: &ValidatedSignup,
        ) -> Result<SignupResult, CreateSignupError> {
            self.signup_result
                .lock()
                .expect("lock poisoned")
                .take()
                .unwrap_or_else(|| {
                    Ok(SignupResult {
                        account_id: Uuid::new_v4(),
                        root_kid: Kid::derive(&[0u8; 32]),
                        device_kid: Kid::derive(&[1u8; 32]),
                    })
                })
        }
    }
}

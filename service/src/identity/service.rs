//! Service layer for identity operations
//!
//! Provides the `SignupService` trait that orchestrates account creation,
//! backup storage, and device key registration as an atomic operation.

use async_trait::async_trait;
use sqlx::PgPool;
use tc_crypto::Kid;
use uuid::Uuid;

use super::repo::{
    create_account_with_executor, create_backup_with_executor, create_device_key_with_executor,
    AccountRepoError, BackupRepoError, DeviceKeyRepoError,
};

/// Validated signup parameters — all validation has passed, ready for persistence.
pub struct ValidatedSignupParams {
    pub username: String,
    pub root_pubkey: String,
    pub root_kid: Kid,
    pub backup_bytes: Vec<u8>,
    pub backup_salt: Vec<u8>,
    pub backup_version: i32,
    pub device_pubkey: String,
    pub device_kid: Kid,
    pub device_name: String,
    pub certificate: Vec<u8>,
}

/// Successful signup result
pub struct SignupResult {
    pub account_id: Uuid,
    pub root_kid: Kid,
    pub device_kid: Kid,
}

/// Error from signup orchestration, tagged by which step failed
#[derive(Debug, thiserror::Error)]
pub enum SignupError {
    #[error("account error: {0}")]
    Account(AccountRepoError),
    #[error("backup error: {0}")]
    Backup(BackupRepoError),
    #[error("device key error: {0}")]
    DeviceKey(DeviceKeyRepoError),
    #[error("internal error: {0}")]
    Internal(String),
}

/// Orchestrates the multi-step signup operation.
#[async_trait]
pub trait SignupService: Send + Sync {
    async fn execute(&self, params: &ValidatedSignupParams) -> Result<SignupResult, SignupError>;
}

/// Production implementation — runs all three inserts in a single transaction.
pub struct PgSignupService {
    pool: PgPool,
}

impl PgSignupService {
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SignupService for PgSignupService {
    async fn execute(&self, p: &ValidatedSignupParams) -> Result<SignupResult, SignupError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| SignupError::Internal(format!("Failed to begin transaction: {e}")))?;

        let account =
            create_account_with_executor(&mut *tx, &p.username, &p.root_pubkey, &p.root_kid)
                .await
                .map_err(SignupError::Account)?;

        create_backup_with_executor(
            &mut *tx,
            account.id,
            &p.root_kid,
            &p.backup_bytes,
            &p.backup_salt,
            p.backup_version,
        )
        .await
        .map_err(SignupError::Backup)?;

        let device = create_device_key_with_executor(
            &mut tx,
            account.id,
            &p.device_kid,
            &p.device_pubkey,
            &p.device_name,
            &p.certificate,
        )
        .await
        .map_err(SignupError::DeviceKey)?;

        tx.commit()
            .await
            .map_err(|e| SignupError::Internal(format!("Failed to commit transaction: {e}")))?;

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
    //! Mock signup service composed of mock repositories for unit testing.

    use super::{async_trait, SignupError, SignupResult, SignupService, ValidatedSignupParams};
    use crate::identity::repo::{
        mock::{MockAccountRepo, MockBackupRepo, MockDeviceKeyRepo},
        AccountRepo, BackupRepo, CreatedAccount, DeviceKeyRepo,
    };

    /// Mock signup service that delegates to individual mock repositories.
    ///
    /// Unlike `PgSignupService`, this does not use a transaction — each mock
    /// repo returns its preset result independently. This is intentional:
    /// unit tests verify error mapping, not transactional atomicity.
    pub struct MockSignupService {
        pub account_repo: MockAccountRepo,
        pub backup_repo: MockBackupRepo,
        pub device_key_repo: MockDeviceKeyRepo,
    }

    impl MockSignupService {
        #[must_use]
        pub const fn new(
            account_repo: MockAccountRepo,
            backup_repo: MockBackupRepo,
            device_key_repo: MockDeviceKeyRepo,
        ) -> Self {
            Self {
                account_repo,
                backup_repo,
                device_key_repo,
            }
        }
    }

    impl Default for MockSignupService {
        fn default() -> Self {
            Self::new(
                MockAccountRepo::new(),
                MockBackupRepo::new(),
                MockDeviceKeyRepo::new(),
            )
        }
    }

    #[async_trait]
    impl SignupService for MockSignupService {
        async fn execute(&self, p: &ValidatedSignupParams) -> Result<SignupResult, SignupError> {
            let account: CreatedAccount = self
                .account_repo
                .create(&p.username, &p.root_pubkey, &p.root_kid)
                .await
                .map_err(SignupError::Account)?;

            self.backup_repo
                .create(
                    account.id,
                    &p.root_kid,
                    &p.backup_bytes,
                    &p.backup_salt,
                    p.backup_version,
                )
                .await
                .map_err(SignupError::Backup)?;

            let device = self
                .device_key_repo
                .create(
                    account.id,
                    &p.device_kid,
                    &p.device_pubkey,
                    &p.device_name,
                    &p.certificate,
                )
                .await
                .map_err(SignupError::DeviceKey)?;

            Ok(SignupResult {
                account_id: account.id,
                root_kid: account.root_kid,
                device_kid: device.device_kid,
            })
        }
    }
}

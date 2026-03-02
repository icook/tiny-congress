//! Repository layer for identity persistence

pub mod accounts;
pub mod backups;
pub mod device_keys;
pub mod identity;
pub mod nonces;

pub use accounts::{
    create_account_with_executor, get_account_by_id, get_account_by_username, AccountRecord,
    AccountRepoError, CreatedAccount,
};
pub use backups::{create_backup_with_executor, BackupRecord, BackupRepoError, CreatedBackup};
pub use device_keys::{
    create_device_key_with_executor, CreatedDeviceKey, DeviceKeyRecord, DeviceKeyRepoError,
};
pub use identity::{
    CreateSignupError, IdentityRepo, PgIdentityRepo, SignupResult, ValidatedSignup,
};
pub use nonces::{check_and_record_nonce, cleanup_expired_nonces, NonceRepoError};

#[cfg(any(test, feature = "test-utils"))]
pub mod mock {
    pub use super::identity::mock::MockIdentityRepo;
}

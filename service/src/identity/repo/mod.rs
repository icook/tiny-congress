//! Repository layer for identity persistence

pub mod accounts;
pub mod backups;
pub mod device_keys;
pub mod identity;

pub use accounts::{
    create_account_with_executor, get_account_by_id, AccountRecord, AccountRepoError,
    CreatedAccount,
};
pub use backups::{create_backup_with_executor, BackupRecord, BackupRepoError, CreatedBackup};
pub use device_keys::{
    create_device_key_with_executor, CreatedDeviceKey, DeviceKeyRecord, DeviceKeyRepoError,
};
pub use identity::{
    CreateSignupError, IdentityRepo, PgIdentityRepo, SignupResult, ValidatedSignup,
};

#[cfg(any(test, feature = "test-utils"))]
pub mod mock {
    pub use super::identity::mock::MockIdentityRepo;
}

//! Repository layer for identity persistence

pub mod accounts;
pub mod backups;
pub mod device_keys;

pub use accounts::{
    create_account_with_executor, AccountRepo, AccountRepoError, CreatedAccount, PgAccountRepo,
};
pub use backups::{
    create_backup_with_executor, BackupRecord, BackupRepo, BackupRepoError, CreatedBackup,
    PgBackupRepo,
};
pub use device_keys::{
    create_device_key_with_executor, CreatedDeviceKey, DeviceKeyRecord, DeviceKeyRepo,
    DeviceKeyRepoError, PgDeviceKeyRepo,
};

// Re-export mock for use in tests across the crate and integration tests
#[cfg(any(test, feature = "test-utils"))]
pub mod mock {
    pub use super::accounts::mock::MockAccountRepo;
    pub use super::backups::mock::MockBackupRepo;
    pub use super::device_keys::mock::MockDeviceKeyRepo;
}

//! Repository layer for identity persistence

pub mod accounts;
pub mod backup;

pub use accounts::{
    create_account_with_executor, AccountRepo, AccountRepoError, CreatedAccount, PgAccountRepo,
};
pub use backup::{BackupRecord, BackupRepo, BackupRepoError, CreatedBackup, PgBackupRepo};

// Re-export mock for use in tests across the crate and integration tests
#[cfg(any(test, feature = "test-utils"))]
pub mod mock {
    pub use super::accounts::mock::MockAccountRepo;
    pub use super::backup::mock::MockBackupRepo;
}

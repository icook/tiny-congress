//! Repository layer for identity persistence

pub mod accounts;

pub use accounts::{
    create_account_with_executor, AccountRepo, AccountRepoError, CreatedAccount, PgAccountRepo,
};

/// Mock implementations for testing.
#[cfg(any(test, feature = "test-utils"))]
pub mod mock {
    pub use super::accounts::mock::MockAccountRepo;
}

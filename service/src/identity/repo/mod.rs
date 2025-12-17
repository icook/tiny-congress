//! Repository layer for identity persistence

pub mod accounts;

pub use accounts::{AccountRepo, AccountRepoError, CreatedAccount, PgAccountRepo};

// Re-export mock for use in tests across the crate
#[cfg(test)]
pub mod mock {
    pub use super::accounts::mock::MockAccountRepo;
}

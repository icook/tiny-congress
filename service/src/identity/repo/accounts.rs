//! Account repository for database operations

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

/// Account creation result
#[derive(Debug, Clone)]
pub struct CreatedAccount {
    pub id: Uuid,
    pub root_kid: String,
}

/// Error types for account operations
#[derive(Debug, thiserror::Error)]
pub enum AccountRepoError {
    #[error("username already taken")]
    DuplicateUsername,
    #[error("public key already registered")]
    DuplicateKey,
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

/// Repository trait for account operations
///
/// This trait abstracts database operations to enable unit testing
/// handlers with mock implementations.
#[async_trait]
pub trait AccountRepo: Send + Sync {
    /// Create a new account with the given credentials
    ///
    /// # Errors
    ///
    /// Returns `AccountRepoError::DuplicateUsername` if username is taken.
    /// Returns `AccountRepoError::DuplicateKey` if public key is already registered.
    async fn create(
        &self,
        username: &str,
        root_pubkey: &str,
        root_kid: &str,
    ) -> Result<CreatedAccount, AccountRepoError>;
}

/// `PostgreSQL` implementation of [`AccountRepo`]
pub struct PgAccountRepo {
    pool: PgPool,
}

impl PgAccountRepo {
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AccountRepo for PgAccountRepo {
    async fn create(
        &self,
        username: &str,
        root_pubkey: &str,
        root_kid: &str,
    ) -> Result<CreatedAccount, AccountRepoError> {
        let id = Uuid::new_v4();

        let result = sqlx::query(
            r"
            INSERT INTO accounts (id, username, root_pubkey, root_kid)
            VALUES ($1, $2, $3, $4)
            ",
        )
        .bind(id)
        .bind(username)
        .bind(root_pubkey)
        .bind(root_kid)
        .execute(&self.pool)
        .await;

        match result {
            Ok(_) => Ok(CreatedAccount {
                id,
                root_kid: root_kid.to_string(),
            }),
            Err(e) => {
                if let sqlx::Error::Database(db_err) = &e {
                    if let Some(constraint) = db_err.constraint() {
                        match constraint {
                            "accounts_username_key" => {
                                return Err(AccountRepoError::DuplicateUsername)
                            }
                            "accounts_root_kid_key" => return Err(AccountRepoError::DuplicateKey),
                            _ => {}
                        }
                    }
                }
                Err(AccountRepoError::Database(e))
            }
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[allow(clippy::expect_used)]
pub mod mock {
    //! Mock implementation for testing

    use super::{async_trait, AccountRepo, AccountRepoError, CreatedAccount, Uuid};
    use std::sync::Mutex;

    /// Mock account repository for unit tests.
    pub struct MockAccountRepo {
        /// Preset result to return from `create()`.
        pub create_result: Mutex<Option<Result<CreatedAccount, AccountRepoError>>>,
    }

    impl MockAccountRepo {
        /// Create a new mock repository.
        #[must_use]
        pub const fn new() -> Self {
            Self {
                create_result: Mutex::new(None),
            }
        }

        /// Set the result that `create()` will return.
        ///
        /// # Panics
        ///
        /// Panics if the internal mutex is poisoned.
        pub fn set_create_result(&self, result: Result<CreatedAccount, AccountRepoError>) {
            *self.create_result.lock().expect("lock poisoned") = Some(result);
        }
    }

    impl Default for MockAccountRepo {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait]
    impl AccountRepo for MockAccountRepo {
        async fn create(
            &self,
            _username: &str,
            _root_pubkey: &str,
            root_kid: &str,
        ) -> Result<CreatedAccount, AccountRepoError> {
            self.create_result
                .lock()
                .expect("lock poisoned")
                .take()
                .unwrap_or_else(|| {
                    Ok(CreatedAccount {
                        id: Uuid::new_v4(),
                        root_kid: root_kid.to_string(),
                    })
                })
        }
    }
}

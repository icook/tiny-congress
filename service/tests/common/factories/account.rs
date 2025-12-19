//! Account factory for test data creation.

use super::next_id;
use tc_crypto::{derive_kid, encode_base64url};
use tinycongress_api::identity::repo::{create_account_with_executor, CreatedAccount};

/// Builder for creating test accounts with sensible defaults.
///
/// # Examples
///
/// ```rust
/// // Create with all defaults
/// let account = AccountFactory::new().create(&mut tx).await;
///
/// // Customize specific fields
/// let account = AccountFactory::new()
///     .with_username("alice")
///     .with_seed(42)
///     .create(&mut tx).await;
/// ```
pub struct AccountFactory {
    username: Option<String>,
    seed: Option<u8>,
}

impl AccountFactory {
    /// Create a new factory with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            username: None,
            seed: None,
        }
    }

    /// Set a specific username.
    #[must_use]
    pub fn with_username(mut self, username: &str) -> Self {
        self.username = Some(username.to_string());
        self
    }

    /// Set a specific seed for key generation.
    /// Different seeds produce different key pairs.
    #[must_use]
    pub fn with_seed(mut self, seed: u8) -> Self {
        self.seed = Some(seed);
        self
    }

    /// Create the account in the database.
    ///
    /// # Panics
    ///
    /// Panics if the database insert fails.
    pub async fn create<'e, E>(self, executor: E) -> CreatedAccount
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        let id = next_id();
        let username = self.username.unwrap_or_else(|| format!("user_{id}"));
        // Safe: id % 256 is guaranteed to be in range 0..=255, which fits in u8
        #[allow(clippy::cast_possible_truncation)]
        let seed = self.seed.unwrap_or((id % 256) as u8);

        let (root_pubkey, root_kid) = generate_test_keys(seed);

        create_account_with_executor(executor, &username, &root_pubkey, &root_kid)
            .await
            .expect("AccountFactory: failed to create account")
    }
}

impl Default for AccountFactory {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate test key pair from a seed byte.
fn generate_test_keys(seed: u8) -> (String, String) {
    let pubkey = [seed; 32];
    let root_pubkey = encode_base64url(&pubkey);
    let root_kid = derive_kid(&pubkey);
    (root_pubkey, root_kid)
}

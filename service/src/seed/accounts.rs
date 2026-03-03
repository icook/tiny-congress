//! Synthetic account creation and endorsement management for demo seeding.

use sqlx::PgPool;
use uuid::Uuid;

use crate::identity::repo::{
    create_account_with_executor, get_account_by_username, AccountRepoError,
};
use crate::reputation::repo::{create_endorsement, ensure_verifier_account, EndorsementRepoError};
use tc_crypto::{encode_base64url, Kid};

/// A synthetic account with its database ID.
#[derive(Debug, Clone)]
pub struct SyntheticAccount {
    pub id: Uuid,
    pub username: String,
}

/// Generate a deterministic public key and KID from a seed byte.
///
/// Uses `[seed; 32]` as the raw public-key bytes, then base64url-encodes
/// them and derives the corresponding [`Kid`].
fn generate_deterministic_keys(seed: u8) -> (String, Kid) {
    let pubkey_bytes = [seed; 32];
    let pubkey = encode_base64url(&pubkey_bytes);
    let kid = Kid::derive(&pubkey_bytes);
    (pubkey, kid)
}

/// Ensure `count` synthetic demo accounts exist in the database.
///
/// Accounts are named `demo_voter_01` through `demo_voter_{count}` with
/// deterministic keys derived from seed `((i + 100) % 256) as u8`. For each
/// account, the function checks whether it already exists and creates it
/// if necessary. Race conditions (duplicate username from a concurrent
/// insert) are handled by re-fetching the existing account.
///
/// # Errors
///
/// Returns an error if a database operation fails for a reason other than
/// a recoverable duplicate.
pub async fn ensure_synthetic_accounts(
    pool: &PgPool,
    count: usize,
) -> Result<Vec<SyntheticAccount>, anyhow::Error> {
    let mut accounts = Vec::with_capacity(count);

    for i in 0..count {
        let username = format!("demo_voter_{:02}", i + 1);
        #[allow(clippy::cast_possible_truncation)] // value is always < 256 after modulo
        let seed = ((i + 100) % 256) as u8;
        let (pubkey, kid) = generate_deterministic_keys(seed);

        let account = match get_account_by_username(pool, &username).await {
            Ok(record) => SyntheticAccount {
                id: record.id,
                username: record.username,
            },
            Err(AccountRepoError::NotFound) => {
                match create_account_with_executor(pool, &username, &pubkey, &kid).await {
                    Ok(created) => {
                        tracing::info!(username = %username, "created synthetic account");
                        SyntheticAccount {
                            id: created.id,
                            username: username.clone(),
                        }
                    }
                    Err(AccountRepoError::DuplicateUsername) => {
                        // Race condition: another worker created it between our
                        // check and insert. Fetch the existing record.
                        let record = get_account_by_username(pool, &username).await?;
                        SyntheticAccount {
                            id: record.id,
                            username: record.username,
                        }
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            Err(e) => return Err(e.into()),
        };

        accounts.push(account);
    }

    Ok(accounts)
}

/// Ensure all synthetic accounts are endorsed for the given topic.
///
/// Creates (or retrieves) a verifier account named `"demo-seeder"` and then
/// issues an endorsement for each account. Accounts that already hold an
/// endorsement for the topic are silently skipped.
///
/// # Errors
///
/// Returns an error if the verifier cannot be created or a database
/// operation fails for a reason other than a duplicate endorsement.
pub async fn ensure_endorsements(
    pool: &PgPool,
    accounts: &[SyntheticAccount],
    topic: &str,
) -> Result<(), anyhow::Error> {
    let verifier = ensure_verifier_account(pool, "demo-seeder", Some("Demo Seed Worker")).await?;

    for account in accounts {
        match create_endorsement(pool, account.id, topic, verifier.id, None).await {
            Ok(_) | Err(EndorsementRepoError::Duplicate) => {
                // Created or already endorsed — either way, move on.
            }
            Err(e) => return Err(e.into()),
        }
    }

    Ok(())
}

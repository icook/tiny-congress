//! Config-driven verifier bootstrap.
//!
//! At startup, ensures each configured verifier has:
//! 1. A user account (created if missing)
//! 2. An `authorized_verifier` endorsement with NULL issuer (genesis)

use sqlx::PgPool;
use uuid::Uuid;

/// Temporary bootstrap for the ID.me verifier as a real account.
///
/// Creates a service account named "idme" if it doesn't already exist,
/// and ensures it has an `authorized_verifier` genesis endorsement.
///
/// This will be replaced by config-driven bootstrap in a later task.
///
/// # Errors
///
/// Returns an error if the database operations fail.
pub async fn bootstrap_idme_verifier_account(pool: &PgPool) -> Result<Uuid, anyhow::Error> {
    // Generate a deterministic-ish keypair placeholder for the service account.
    // In the config-driven version, this comes from TC_VERIFIERS config.
    let name = "idme";

    // Try to find existing account by username
    let existing = sqlx::query_scalar::<_, Uuid>("SELECT id FROM accounts WHERE username = $1")
        .bind(name)
        .fetch_optional(pool)
        .await?;

    let account_id = if let Some(id) = existing {
        id
    } else {
        // Create a placeholder service account.
        // The root_pubkey and root_kid are placeholders — the config-driven
        // bootstrap (Task 6) will use real keys from TC_VERIFIERS config.
        let id = Uuid::new_v4();
        let placeholder_pubkey = tc_crypto::encode_base64url(&[0u8; 32]);
        let placeholder_kid = tc_crypto::Kid::derive(&[0u8; 32]);

        sqlx::query(
            r"INSERT INTO accounts (id, username, root_pubkey, root_kid)
              VALUES ($1, $2, $3, $4)
              ON CONFLICT (username) DO UPDATE SET username = EXCLUDED.username
              RETURNING id",
        )
        .bind(id)
        .bind(name)
        .bind(&placeholder_pubkey)
        .bind(placeholder_kid.as_str())
        .execute(pool)
        .await?;

        id
    };

    // Ensure authorized_verifier endorsement exists (genesis, NULL issuer)
    sqlx::query(
        r"INSERT INTO reputation__endorsements (id, subject_id, topic, issuer_id)
          VALUES (gen_random_uuid(), $1, 'authorized_verifier', NULL)
          ON CONFLICT (subject_id, topic) WHERE issuer_id IS NULL DO NOTHING",
    )
    .bind(account_id)
    .execute(pool)
    .await?;

    tracing::info!(account_id = %account_id, "ID.me verifier account bootstrapped");
    Ok(account_id)
}

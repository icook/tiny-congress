//! Config-driven verifier bootstrap.
//!
//! At startup, ensures each configured verifier has:
//! 1. A user account (created if missing)
//! 2. An `authorized_verifier` endorsement with NULL issuer (genesis)

use sqlx::PgPool;
use uuid::Uuid;

use crate::config::VerifierConfig;

pub struct BootstrappedVerifier {
    pub name: String,
    pub account_id: Uuid,
}

/// Bootstrap all configured verifiers. Idempotent — safe to call on every startup.
///
/// # Errors
///
/// Returns an error if any database operation fails.
pub async fn bootstrap_verifiers(
    pool: &PgPool,
    verifiers: &[VerifierConfig],
) -> Result<Vec<BootstrappedVerifier>, anyhow::Error> {
    let mut result = Vec::with_capacity(verifiers.len());

    for v in verifiers {
        let account_id = ensure_verifier_account(pool, &v.name, &v.public_key).await?;
        ensure_authorized_verifier_endorsement(pool, account_id).await?;
        tracing::info!(name = %v.name, account_id = %account_id, "Verifier bootstrapped");
        result.push(BootstrappedVerifier {
            name: v.name.clone(),
            account_id,
        });
    }

    Ok(result)
}

/// Ensure an account exists for this verifier. Returns the `account_id`.
async fn ensure_verifier_account(
    pool: &PgPool,
    name: &str,
    public_key: &str,
) -> Result<Uuid, anyhow::Error> {
    // Decode and derive KID from public key
    let pubkey_bytes = tc_crypto::decode_base64url(public_key)
        .map_err(|e| anyhow::anyhow!("Invalid verifier public key for {name}: {e}"))?;
    let kid = tc_crypto::Kid::derive(&pubkey_bytes);

    // Try to find existing account by root_kid
    let existing = sqlx::query_scalar::<_, Uuid>("SELECT id FROM accounts WHERE root_kid = $1")
        .bind(kid.as_str())
        .fetch_optional(pool)
        .await?;

    if let Some(id) = existing {
        return Ok(id);
    }

    // Create new account, returning the actual id (existing or newly inserted)
    let id = Uuid::new_v4();
    let returned_id: (Uuid,) = sqlx::query_as(
        r"INSERT INTO accounts (id, username, root_pubkey, root_kid)
          VALUES ($1, $2, $3, $4)
          ON CONFLICT (username) DO UPDATE SET root_pubkey = EXCLUDED.root_pubkey, root_kid = EXCLUDED.root_kid
          RETURNING id",
    )
    .bind(id)
    .bind(name)
    .bind(public_key)
    .bind(kid.as_str())
    .fetch_one(pool)
    .await?;

    Ok(returned_id.0)
}

/// Ensure the account has an `authorized_verifier` endorsement (genesis, NULL issuer).
async fn ensure_authorized_verifier_endorsement(
    pool: &PgPool,
    account_id: Uuid,
) -> Result<(), anyhow::Error> {
    sqlx::query(
        r"INSERT INTO reputation__endorsements (id, subject_id, topic, issuer_id)
          VALUES (gen_random_uuid(), $1, 'authorized_verifier', NULL)
          ON CONFLICT (subject_id, topic) WHERE issuer_id IS NULL DO NOTHING",
    )
    .bind(account_id)
    .execute(pool)
    .await?;

    Ok(())
}

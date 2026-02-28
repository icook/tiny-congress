//! Nonce repository for replay prevention
//!
//! Stores SHA-256 hashes of request nonces in postgres. A nonce that has
//! already been recorded is rejected as a replay. A background task
//! periodically deletes entries older than the timestamp skew window.

use sqlx::PgPool;

/// Errors from nonce operations.
#[derive(Debug, thiserror::Error)]
pub enum NonceRepoError {
    #[error("request replay detected")]
    Replay,
    #[error("database error: {0}")]
    Database(sqlx::Error),
}

/// Record a nonce hash. Returns `NonceRepoError::Replay` if already seen.
///
/// # Errors
///
/// - [`NonceRepoError::Replay`] if the nonce was already recorded
/// - [`NonceRepoError::Database`] on connection or query failure
pub async fn check_and_record_nonce(
    pool: &PgPool,
    nonce_hash: &[u8],
) -> Result<(), NonceRepoError> {
    let result =
        sqlx::query("INSERT INTO request_nonces (nonce_hash) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(nonce_hash)
            .execute(pool)
            .await
            .map_err(NonceRepoError::Database)?;

    if result.rows_affected() == 0 {
        return Err(NonceRepoError::Replay);
    }
    Ok(())
}

/// Delete nonces older than `max_age_secs`. Returns count of deleted rows.
///
/// # Errors
///
/// Returns [`NonceRepoError::Database`] on connection or query failure.
pub async fn cleanup_expired_nonces(
    pool: &PgPool,
    max_age_secs: i64,
) -> Result<u64, NonceRepoError> {
    let result = sqlx::query(
        "DELETE FROM request_nonces WHERE created_at < now() - make_interval(secs => $1::float8)",
    )
    .bind(max_age_secs)
    .execute(pool)
    .await
    .map_err(NonceRepoError::Database)?;

    Ok(result.rows_affected())
}

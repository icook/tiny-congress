use sqlx::PgPool;
use uuid::Uuid;

use super::{DenouncementRecord, TrustRepoError};

pub(super) async fn create_denouncement(
    pool: &PgPool,
    accuser_id: Uuid,
    target_id: Uuid,
    reason: &str,
) -> Result<DenouncementRecord, TrustRepoError> {
    sqlx::query_as::<_, DenouncementRecord>(
        "INSERT INTO trust__denouncements (accuser_id, target_id, reason) \
         VALUES ($1, $2, $3) \
         RETURNING *",
    )
    .bind(accuser_id)
    .bind(target_id)
    .bind(reason)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e {
            if db_err.constraint() == Some("uq_denouncement_accuser_target") {
                return TrustRepoError::Duplicate;
            }
        }
        TrustRepoError::Database(e)
    })
}

pub(super) async fn list_denouncements_against(
    pool: &PgPool,
    target_id: Uuid,
) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
    let records = sqlx::query_as::<_, DenouncementRecord>(
        "SELECT * FROM trust__denouncements \
         WHERE target_id = $1 \
         ORDER BY created_at DESC",
    )
    .bind(target_id)
    .fetch_all(pool)
    .await?;

    Ok(records)
}

/// Returns `true` if `accuser_id` has an active (non-resolved) denouncement against `target_id`.
pub(super) async fn has_active_denouncement(
    pool: &PgPool,
    accuser_id: Uuid,
    target_id: Uuid,
) -> Result<bool, TrustRepoError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM trust__denouncements \
         WHERE accuser_id = $1 AND target_id = $2 AND resolved_at IS NULL",
    )
    .bind(accuser_id)
    .bind(target_id)
    .fetch_one(pool)
    .await?;

    Ok(count > 0)
}

/// Count total denouncements filed by `accuser_id` (permanent budget — no refunds).
pub(super) async fn count_active_denouncements_by(
    pool: &PgPool,
    accuser_id: Uuid,
) -> Result<i64, TrustRepoError> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM trust__denouncements \
         WHERE accuser_id = $1",
    )
    .bind(accuser_id)
    .fetch_one(pool)
    .await?;

    Ok(count)
}

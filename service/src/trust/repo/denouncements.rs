use sqlx::PgPool;
use uuid::Uuid;

use super::{DenouncementRecord, TrustRepoError};

pub(super) async fn create_denouncement<'e, E>(
    executor: E,
    accuser_id: Uuid,
    target_id: Uuid,
    reason: &str,
) -> Result<DenouncementRecord, TrustRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, DenouncementRecord>(
        "INSERT INTO trust__denouncements (accuser_id, target_id, reason) \
         VALUES ($1, $2, $3) \
         RETURNING *",
    )
    .bind(accuser_id)
    .bind(target_id)
    .bind(reason)
    .fetch_one(executor)
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

/// Atomically insert a denouncement and revoke any active trust endorsement from
/// `accuser_id` to `target_id` in a single transaction.
///
/// If the denouncement already exists (`Duplicate`), the error is returned and the
/// transaction is rolled back without touching the endorsement table.
pub(super) async fn create_denouncement_and_revoke_endorsement(
    pool: &PgPool,
    accuser_id: Uuid,
    target_id: Uuid,
    reason: &str,
) -> Result<DenouncementRecord, TrustRepoError> {
    let mut tx = pool.begin().await?;

    let record = create_denouncement(&mut *tx, accuser_id, target_id, reason).await?;

    // Revoke any active trust endorsement from accuser → target.  This is a
    // no-op when no such endorsement exists, so we don't treat it as an error.
    crate::reputation::repo::endorsements::revoke_endorsement(
        &mut *tx, accuser_id, target_id, "trust",
    )
    .await
    .map_err(|e| match e {
        crate::reputation::repo::endorsements::EndorsementRepoError::Database(db_err) => {
            TrustRepoError::Database(db_err)
        }
        crate::reputation::repo::endorsements::EndorsementRepoError::NotFound => {
            TrustRepoError::NotFound
        }
    })?;

    tx.commit().await?;

    Ok(record)
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

pub(super) async fn list_denouncements_by(
    pool: &PgPool,
    accuser_id: Uuid,
) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
    let records = sqlx::query_as::<_, DenouncementRecord>(
        "SELECT * FROM trust__denouncements \
         WHERE accuser_id = $1 \
         ORDER BY created_at DESC",
    )
    .bind(accuser_id)
    .fetch_all(pool)
    .await?;

    Ok(records)
}

pub(super) async fn list_denouncements_by_with_username(
    pool: &PgPool,
    accuser_id: Uuid,
) -> Result<Vec<super::DenouncementWithUsername>, TrustRepoError> {
    let records = sqlx::query_as::<_, super::DenouncementWithUsername>(
        "SELECT d.id, d.target_id, a.username AS target_username, d.reason, d.created_at \
         FROM trust__denouncements d \
         JOIN accounts a ON a.id = d.target_id \
         WHERE d.accuser_id = $1 \
         ORDER BY d.created_at DESC",
    )
    .bind(accuser_id)
    .fetch_all(pool)
    .await?;
    Ok(records)
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

use sqlx::PgPool;
use uuid::Uuid;

use super::{ActionRecord, TrustRepoError};

pub(super) async fn enqueue_action(
    pool: &PgPool,
    actor_id: Uuid,
    action_type: &str,
    payload: &serde_json::Value,
) -> Result<ActionRecord, TrustRepoError> {
    let record = sqlx::query_as::<_, ActionRecord>(
        "INSERT INTO trust__action_queue (actor_id, action_type, payload) \
         VALUES ($1, $2, $3) \
         RETURNING *",
    )
    .bind(actor_id)
    .bind(action_type)
    .bind(payload)
    .fetch_one(pool)
    .await?;

    Ok(record)
}

pub(super) async fn count_daily_actions(
    pool: &PgPool,
    actor_id: Uuid,
) -> Result<i64, TrustRepoError> {
    let row = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM trust__action_queue \
         WHERE actor_id = $1 AND quota_date = CURRENT_DATE",
    )
    .bind(actor_id)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

pub(super) async fn claim_pending_actions(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<ActionRecord>, TrustRepoError> {
    let records = sqlx::query_as::<_, ActionRecord>(
        "UPDATE trust__action_queue \
         SET status = 'processing' \
         WHERE id IN ( \
             SELECT id FROM trust__action_queue \
             WHERE status = 'pending' \
             ORDER BY created_at ASC \
             LIMIT $1 \
             FOR UPDATE SKIP LOCKED \
         ) \
         RETURNING *",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(records)
}

pub(super) async fn complete_action(pool: &PgPool, action_id: Uuid) -> Result<(), TrustRepoError> {
    sqlx::query(
        "UPDATE trust__action_queue \
         SET status = 'completed', processed_at = now() \
         WHERE id = $1",
    )
    .bind(action_id)
    .execute(pool)
    .await?;

    Ok(())
}

pub(super) async fn fail_action(
    pool: &PgPool,
    action_id: Uuid,
    error: &str,
) -> Result<(), TrustRepoError> {
    sqlx::query(
        "UPDATE trust__action_queue \
         SET status = 'failed', error_message = $2, processed_at = now() \
         WHERE id = $1",
    )
    .bind(action_id)
    .bind(error)
    .execute(pool)
    .await?;

    Ok(())
}

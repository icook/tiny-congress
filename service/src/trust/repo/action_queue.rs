use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use tc_engine_polling::repo::pgmq;

use super::super::service::ActionType;
use super::{ActionRecord, TrustRepoError};

/// pgmq queue name for trust actions.
pub const QUEUE_NAME: &str = "trust__actions";

pub(super) async fn enqueue_action(
    pool: &PgPool,
    actor_id: Uuid,
    action_type: ActionType,
    payload: &serde_json::Value,
) -> Result<ActionRecord, TrustRepoError> {
    let record = sqlx::query_as::<_, ActionRecord>(
        "INSERT INTO trust__action_log (actor_id, action_type, payload) \
         VALUES ($1, $2, $3) \
         RETURNING *",
    )
    .bind(actor_id)
    .bind(action_type.as_str())
    .bind(payload)
    .fetch_one(pool)
    .await?;

    let msg_payload = json!({ "log_id": record.id.to_string() });
    pgmq::send(pool, QUEUE_NAME, &msg_payload)
        .await
        .map_err(TrustRepoError::Database)?;

    Ok(record)
}

pub(super) async fn count_daily_actions(
    pool: &PgPool,
    actor_id: Uuid,
) -> Result<i64, TrustRepoError> {
    let row = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM trust__action_log \
         WHERE actor_id = $1 AND quota_date = CURRENT_DATE",
    )
    .bind(actor_id)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

pub(super) async fn get_action(
    pool: &PgPool,
    action_id: Uuid,
) -> Result<ActionRecord, TrustRepoError> {
    sqlx::query_as::<_, ActionRecord>("SELECT * FROM trust__action_log WHERE id = $1")
        .bind(action_id)
        .fetch_optional(pool)
        .await?
        .ok_or(TrustRepoError::NotFound)
}

pub(super) async fn complete_action(pool: &PgPool, action_id: Uuid) -> Result<(), TrustRepoError> {
    sqlx::query(
        "UPDATE trust__action_log \
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
        "UPDATE trust__action_log \
         SET status = 'failed', error_message = $2, processed_at = now() \
         WHERE id = $1",
    )
    .bind(action_id)
    .bind(error)
    .execute(pool)
    .await?;

    Ok(())
}

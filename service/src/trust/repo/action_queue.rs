use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use tc_engine_polling::repo::pgmq;

use super::super::service::ActionType;
use super::{ActionRecord, TrustRepoError};

/// pgmq queue name for trust actions.
pub const QUEUE_NAME: &str = "trust__actions";

/// Maximum length (in Unicode scalar values) stored in the `error_message` column.
///
/// Error strings sourced from `sqlx::Error` or nested engine errors can include full
/// query text and driver context, easily exceeding kilobytes.  Truncating here keeps
/// rows bounded without a schema migration.
pub const ERROR_MESSAGE_MAX_LEN: usize = 1024;

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

/// Truncate `error` to at most [`ERROR_MESSAGE_MAX_LEN`] Unicode scalar values,
/// always cutting at a character boundary to avoid splitting multi-byte sequences.
pub(crate) fn truncate_error_message(error: &str) -> &str {
    let byte_end = error
        .char_indices()
        .nth(ERROR_MESSAGE_MAX_LEN)
        .map_or(error.len(), |(i, _)| i);
    &error[..byte_end]
}

pub(super) async fn fail_action(
    pool: &PgPool,
    action_id: Uuid,
    error: &str,
) -> Result<(), TrustRepoError> {
    let error = truncate_error_message(error);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_error_message_passes_short_error_unchanged() {
        let error = "short error";
        assert_eq!(truncate_error_message(error), error);
    }

    #[test]
    fn truncate_error_message_passes_exactly_max_len_unchanged() {
        let error = "x".repeat(ERROR_MESSAGE_MAX_LEN);
        assert_eq!(truncate_error_message(&error), error);
    }

    #[test]
    fn truncate_error_message_truncates_to_max_len() {
        let error = "x".repeat(ERROR_MESSAGE_MAX_LEN + 10);
        let result = truncate_error_message(&error);
        assert_eq!(result.chars().count(), ERROR_MESSAGE_MAX_LEN);
    }

    #[test]
    fn truncate_error_message_cuts_at_char_boundary_for_multibyte() {
        // Each '中' is 3 bytes; build a string that exceeds the limit by one char.
        let error = "中".repeat(ERROR_MESSAGE_MAX_LEN + 1);
        let result = truncate_error_message(&error);
        assert_eq!(result.chars().count(), ERROR_MESSAGE_MAX_LEN);
        // Must be valid UTF-8 — slicing inside a multibyte char would panic.
        assert!(std::str::from_utf8(result.as_bytes()).is_ok());
    }

    #[test]
    fn truncate_error_message_returns_empty_for_empty_input() {
        assert_eq!(truncate_error_message(""), "");
    }
}

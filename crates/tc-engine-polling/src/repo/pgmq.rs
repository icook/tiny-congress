//! pgmq-backed bot task queue persistence operations
//!
//! Provides a thin wrapper around pgmq SQL functions for the `rooms__bot_tasks`
//! queue. Messages are enqueued with `pgmq.send`, read with `pgmq.read`, and
//! disposed of via `pgmq.delete` (processed) or `pgmq.archive` (audit trail).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

/// The pgmq queue name for bot tasks.
pub const QUEUE_NAME: &str = "rooms__bot_tasks";

// ─── Payload types ──────────────────────────────────────────────────────────

/// Message payload enqueued for a bot task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotTask {
    /// The room this task belongs to.
    pub room_id: Uuid,
    /// The task discriminant (e.g. `"run_bot"`).
    pub task: String,
    /// Arbitrary task-specific parameters.
    pub params: serde_json::Value,
}

/// A message returned by `pgmq.read`.
#[derive(Debug, Clone)]
pub struct PgmqMessage {
    /// pgmq-assigned message identifier; used for delete/archive.
    pub msg_id: i64,
    /// Number of times this message has been read (delivery attempts).
    pub read_ct: i32,
    /// When the message was first enqueued.
    pub enqueued_at: DateTime<Utc>,
    /// Visibility timeout — message is hidden until this timestamp.
    pub vt: DateTime<Utc>,
    /// Raw JSON payload.
    pub message: serde_json::Value,
}

#[derive(sqlx::FromRow)]
struct PgmqRow {
    msg_id: i64,
    read_ct: i32,
    enqueued_at: DateTime<Utc>,
    vt: DateTime<Utc>,
    message: serde_json::Value,
}

impl From<PgmqRow> for PgmqMessage {
    fn from(row: PgmqRow) -> Self {
        Self {
            msg_id: row.msg_id,
            read_ct: row.read_ct,
            enqueued_at: row.enqueued_at,
            vt: row.vt,
            message: row.message,
        }
    }
}

// ─── Generic queue operations ────────────────────────────────────────────────

/// Enqueue a raw JSON payload onto any named queue and return the message ID.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection error.
pub async fn send(
    pool: &PgPool,
    queue_name: &str,
    payload: &serde_json::Value,
) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT * FROM pgmq.send($1, $2)")
        .bind(queue_name)
        .bind(payload)
        .fetch_one(pool)
        .await?;

    Ok(row.0)
}

/// Enqueue a raw JSON payload with a visibility delay and return the message ID.
///
/// The message will not be visible for reading until `delay_secs` seconds
/// have elapsed.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection error.
pub async fn send_delayed(
    pool: &PgPool,
    queue_name: &str,
    payload: &serde_json::Value,
    delay_secs: i32,
) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT * FROM pgmq.send($1, $2, $3)")
        .bind(queue_name)
        .bind(payload)
        .bind(delay_secs)
        .fetch_one(pool)
        .await?;

    Ok(row.0)
}

/// Read one message from a named queue, hiding it for `visibility_timeout_secs` seconds.
///
/// Returns `None` when the queue is empty. The message remains hidden from
/// other consumers until the visibility timeout elapses or it is
/// deleted/archived.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn read(
    pool: &PgPool,
    queue_name: &str,
    visibility_timeout_secs: i32,
) -> Result<Option<PgmqMessage>, sqlx::Error> {
    let row = sqlx::query_as::<_, PgmqRow>(
        "SELECT msg_id, read_ct, enqueued_at, vt, message FROM pgmq.read($1, $2, 1)",
    )
    .bind(queue_name)
    .bind(visibility_timeout_secs)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(PgmqMessage::from))
}

/// Delete a message from a named queue after successful processing.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn delete(pool: &PgPool, queue_name: &str, msg_id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pgmq.delete($1, $2)")
        .bind(queue_name)
        .bind(msg_id)
        .execute(pool)
        .await?;

    Ok(())
}

/// Archive a message from a named queue for audit trail retention.
///
/// Moves the message to the pgmq archive table instead of deleting it.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn archive(pool: &PgPool, queue_name: &str, msg_id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pgmq.archive($1, $2)")
        .bind(queue_name)
        .bind(msg_id)
        .execute(pool)
        .await?;

    Ok(())
}

// ─── BotTask convenience wrappers ────────────────────────────────────────────

/// Enqueue a bot task and return the assigned message ID.
///
/// # Errors
///
/// Returns `sqlx::Error` on serialization failure or connection error.
pub async fn send_task(pool: &PgPool, task: &BotTask) -> Result<i64, sqlx::Error> {
    let payload = serde_json::to_value(task)
        .map_err(|e| sqlx::Error::Protocol(format!("failed to serialize BotTask: {e}")))?;

    send(pool, QUEUE_NAME, &payload).await
}

/// Enqueue a bot task with a visibility delay and return the assigned message ID.
///
/// The message will not be visible for reading until `delay_secs` seconds
/// have elapsed.
///
/// # Errors
///
/// Returns `sqlx::Error` on serialization failure or connection error.
pub async fn send_task_delayed(
    pool: &PgPool,
    task: &BotTask,
    delay_secs: i32,
) -> Result<i64, sqlx::Error> {
    let payload = serde_json::to_value(task)
        .map_err(|e| sqlx::Error::Protocol(format!("failed to serialize BotTask: {e}")))?;

    send_delayed(pool, QUEUE_NAME, &payload, delay_secs).await
}

/// Read one message from the bot task queue, hiding it for `visibility_timeout_secs` seconds.
///
/// Returns `None` when the queue is empty. The message remains hidden from
/// other consumers until the visibility timeout elapses or it is
/// deleted/archived.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn read_task(
    pool: &PgPool,
    visibility_timeout_secs: i32,
) -> Result<Option<PgmqMessage>, sqlx::Error> {
    read(pool, QUEUE_NAME, visibility_timeout_secs).await
}

/// Delete a message from the bot task queue after successful processing.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn delete_task(pool: &PgPool, msg_id: i64) -> Result<(), sqlx::Error> {
    delete(pool, QUEUE_NAME, msg_id).await
}

/// Archive a bot task message for audit trail retention.
///
/// Moves the message to the pgmq archive table instead of deleting it.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn archive_task(pool: &PgPool, msg_id: i64) -> Result<(), sqlx::Error> {
    archive(pool, QUEUE_NAME, msg_id).await
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bot_task_serializes_and_deserializes() {
        let task = BotTask {
            room_id: Uuid::nil(),
            task: "run_bot".to_string(),
            params: json!({ "model": "gpt-4o", "temperature": 0.7 }),
        };

        let serialized = serde_json::to_value(&task).expect("serialize");
        assert_eq!(
            serialized["room_id"],
            json!("00000000-0000-0000-0000-000000000000")
        );
        assert_eq!(serialized["task"], json!("run_bot"));
        assert_eq!(serialized["params"]["model"], json!("gpt-4o"));

        let roundtripped: BotTask = serde_json::from_value(serialized).expect("deserialize");
        assert_eq!(roundtripped.room_id, Uuid::nil());
        assert_eq!(roundtripped.task, "run_bot");
        assert_eq!(roundtripped.params["temperature"], json!(0.7));
    }

    #[test]
    fn bot_task_empty_params_roundtrips() {
        let task = BotTask {
            room_id: Uuid::new_v4(),
            task: "ping".to_string(),
            params: json!({}),
        };
        let serialized = serde_json::to_value(&task).expect("serialize");
        let roundtripped: BotTask = serde_json::from_value(serialized).expect("deserialize");
        assert_eq!(roundtripped.task, "ping");
        assert!(roundtripped.params.as_object().unwrap().is_empty());
    }
}

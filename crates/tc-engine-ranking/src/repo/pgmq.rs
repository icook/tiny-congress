//! Thin pgmq wrappers shared by ranking queue operations.
//!
//! Mirrors the pattern from `tc-engine-polling/src/repo/pgmq.rs`.  Each queue
//! module in this crate builds on these generic helpers rather than calling
//! raw SQL directly.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;

// ─── Message type ────────────────────────────────────────────────────────────

/// A message returned by `pgmq.read`.
#[derive(Debug, Clone)]
pub struct PgmqMessage {
    pub msg_id: i64,
    pub read_ct: i32,
    pub enqueued_at: DateTime<Utc>,
    pub message: Value,
}

#[derive(sqlx::FromRow)]
struct PgmqRow {
    msg_id: i64,
    read_ct: i32,
    enqueued_at: DateTime<Utc>,
    message: Value,
}

impl From<PgmqRow> for PgmqMessage {
    fn from(row: PgmqRow) -> Self {
        Self {
            msg_id: row.msg_id,
            read_ct: row.read_ct,
            enqueued_at: row.enqueued_at,
            message: row.message,
        }
    }
}

// ─── Generic queue operations ────────────────────────────────────────────────

/// Enqueue a JSON payload onto a named queue.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection error.
pub async fn send(pool: &PgPool, queue_name: &str, payload: &Value) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT * FROM pgmq.send($1, $2)")
        .bind(queue_name)
        .bind(payload)
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}

/// Enqueue a JSON payload with a visibility delay.
///
/// The message will not be visible until `delay_secs` seconds have elapsed.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection error.
pub async fn send_delayed(
    pool: &PgPool,
    queue_name: &str,
    payload: &Value,
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

/// Read one message from a named queue.
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
        "SELECT msg_id, read_ct, enqueued_at, message \
         FROM pgmq.read($1, $2, 1)",
    )
    .bind(queue_name)
    .bind(visibility_timeout_secs)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(PgmqMessage::from))
}

/// Archive a message for audit trail retention.
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

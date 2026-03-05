//! Lifecycle message queue persistence operations
//!
//! Provides a lightweight job queue backed by `rooms__lifecycle_queue`.
//! Messages become visible after a configurable delay and are consumed
//! atomically via `SELECT FOR UPDATE SKIP LOCKED` + `DELETE`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

// ─── Payload types ──────────────────────────────────────────────────────────

/// Tagged payload for lifecycle queue messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum LifecyclePayload {
    /// Close a specific poll after its timer expires.
    #[serde(rename = "close_poll")]
    ClosePoll { poll_id: Uuid, room_id: Uuid },
    /// Activate the next agenda item for a room.
    #[serde(rename = "activate_next")]
    ActivateNext { room_id: Uuid },
}

/// A message read from the lifecycle queue.
#[derive(Debug, Clone)]
pub struct LifecycleMessage {
    pub id: i64,
    pub payload: LifecyclePayload,
    pub created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct QueueRow {
    id: i64,
    #[allow(dead_code)]
    message_type: String,
    payload: serde_json::Value,
    created_at: DateTime<Utc>,
}

fn row_to_message(row: QueueRow) -> Result<LifecycleMessage, sqlx::Error> {
    let payload: LifecyclePayload = serde_json::from_value(row.payload)
        .map_err(|e| sqlx::Error::Protocol(format!("invalid lifecycle payload: {e}")))?;
    Ok(LifecycleMessage {
        id: row.id,
        payload,
        created_at: row.created_at,
    })
}

// ─── Queue operations ───────────────────────────────────────────────────────

/// Enqueue a lifecycle event with a visibility delay.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn enqueue_lifecycle_event<'e, E>(
    executor: E,
    payload: &LifecyclePayload,
    delay_secs: f64,
) -> Result<(), sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let message_type = match payload {
        LifecyclePayload::ClosePoll { .. } => "close_poll",
        LifecyclePayload::ActivateNext { .. } => "activate_next",
    };
    let json_payload = serde_json::to_value(payload)
        .map_err(|e| sqlx::Error::Protocol(format!("failed to serialize payload: {e}")))?;

    sqlx::query(
        r"
        INSERT INTO rooms__lifecycle_queue (message_type, payload, visible_at)
        VALUES ($1, $2, now() + make_interval(secs => $3::double precision))
        ",
    )
    .bind(message_type)
    .bind(json_payload)
    .bind(delay_secs)
    .execute(executor)
    .await?;

    Ok(())
}

/// Atomically pop the next visible message from the queue.
///
/// Uses `SELECT FOR UPDATE SKIP LOCKED` within a transaction so multiple
/// consumers can run concurrently without double-processing.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn read_lifecycle_event(pool: &PgPool) -> Result<Option<LifecycleMessage>, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let row = sqlx::query_as::<_, QueueRow>(
        r"
        SELECT id, message_type, payload, created_at
        FROM rooms__lifecycle_queue
        WHERE visible_at <= now()
        ORDER BY visible_at, id
        LIMIT 1
        FOR UPDATE SKIP LOCKED
        ",
    )
    .fetch_optional(&mut *tx)
    .await?;

    let Some(row) = row else {
        tx.commit().await?;
        return Ok(None);
    };

    let message_id = row.id;
    let message = row_to_message(row)?;

    sqlx::query("DELETE FROM rooms__lifecycle_queue WHERE id = $1")
        .bind(message_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(Some(message))
}

/// Delete a specific lifecycle event by ID.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn delete_lifecycle_event<'e, E>(executor: E, message_id: i64) -> Result<(), sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query("DELETE FROM rooms__lifecycle_queue WHERE id = $1")
        .bind(message_id)
        .execute(executor)
        .await?;

    Ok(())
}

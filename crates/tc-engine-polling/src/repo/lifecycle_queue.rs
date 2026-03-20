//! Lifecycle queue persistence — pgmq-backed

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use super::pgmq;

/// pgmq queue name for lifecycle events.
pub const QUEUE_NAME: &str = "rooms__lifecycle";

/// Maximum delivery attempts before a message is treated as poison.
const MAX_RETRIES: i32 = 3;

/// Visibility timeout in seconds.
const VISIBILITY_TIMEOUT_SECS: i32 = 60;

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
    /// pgmq message ID — needed for archive/delete.
    pub msg_id: i64,
    /// Number of delivery attempts.
    pub read_ct: i32,
    pub payload: LifecyclePayload,
    pub enqueued_at: DateTime<Utc>,
}

// ─── Queue operations ───────────────────────────────────────────────────────

/// Enqueue a lifecycle event with a visibility delay.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn enqueue_lifecycle_event(
    pool: &PgPool,
    payload: &LifecyclePayload,
    delay_secs: f64,
) -> Result<(), sqlx::Error> {
    let json_payload = serde_json::to_value(payload)
        .map_err(|e| sqlx::Error::Protocol(format!("failed to serialize payload: {e}")))?;

    #[allow(clippy::cast_possible_truncation)]
    let delay = delay_secs as i32;
    if delay > 0 {
        pgmq::send_delayed(pool, QUEUE_NAME, &json_payload, delay).await?;
    } else {
        pgmq::send(pool, QUEUE_NAME, &json_payload).await?;
    }
    Ok(())
}

/// Read one lifecycle message from the queue.
///
/// The message remains hidden from other consumers until the visibility timeout
/// elapses or it is archived.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn read_lifecycle_event(pool: &PgPool) -> Result<Option<LifecycleMessage>, sqlx::Error> {
    let Some(msg) = pgmq::read(pool, QUEUE_NAME, VISIBILITY_TIMEOUT_SECS).await? else {
        return Ok(None);
    };

    let payload: LifecyclePayload = serde_json::from_value(msg.message)
        .map_err(|e| sqlx::Error::Protocol(format!("invalid lifecycle payload: {e}")))?;

    Ok(Some(LifecycleMessage {
        msg_id: msg.msg_id,
        read_ct: msg.read_ct,
        payload,
        enqueued_at: msg.enqueued_at,
    }))
}

/// Archive a lifecycle message after successful processing.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn archive_lifecycle_event(pool: &PgPool, msg_id: i64) -> Result<(), sqlx::Error> {
    pgmq::archive(pool, QUEUE_NAME, msg_id).await
}

/// Check if a message has exceeded the retry limit.
#[must_use]
pub const fn is_poison(msg: &LifecycleMessage) -> bool {
    msg.read_ct > MAX_RETRIES
}

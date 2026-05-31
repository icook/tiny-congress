//! Background consumer for the ranking lifecycle message queue.
//!
//! The lifecycle queue drives the round state machine:
//! `OpenSubmit` → `OpenRanking` → `CloseRound` → `OpenSubmit` …
//!
//! Each message is enqueued with a delay so the transition fires at the right
//! wall-clock time.  On failure the message is left in the queue and will
//! redeliver after the visibility timeout.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use tc_engine_api::engine::RoomLifecycle;

use crate::config::RankingConfig;
use crate::repo::pgmq;
use crate::service::RankingService;

// ─── Constants ───────────────────────────────────────────────────────────────

/// pgmq queue name for ranking lifecycle events.
pub const QUEUE_NAME: &str = "rooms__ranking_lifecycle";

/// Maximum delivery attempts before a message is treated as poison.
const MAX_RETRIES: i32 = 3;

/// Visibility timeout in seconds — how long a message stays hidden on read.
const VISIBILITY_TIMEOUT_SECS: i32 = 60;

// ─── Payload types ───────────────────────────────────────────────────────────

/// Tagged payload for ranking lifecycle queue messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RankingLifecyclePayload {
    /// Open the submission phase for a room. Creates a new round.
    #[serde(rename = "open_submit")]
    OpenSubmit { room_id: Uuid },
    /// Transition a round from submission phase to ranking phase.
    #[serde(rename = "open_ranking")]
    OpenRanking { round_id: Uuid, room_id: Uuid },
    /// Close a round and snapshot the hall of fame.
    #[serde(rename = "close_round")]
    CloseRound { round_id: Uuid, room_id: Uuid },
}

/// A message read from the ranking lifecycle queue.
#[derive(Debug, Clone)]
pub struct RankingLifecycleMessage {
    /// pgmq message ID — needed for archive/delete.
    pub msg_id: i64,
    /// Number of delivery attempts.
    pub read_ct: i32,
    pub payload: RankingLifecyclePayload,
    pub enqueued_at: DateTime<Utc>,
}

// ─── Queue operations ────────────────────────────────────────────────────────

/// Enqueue a ranking lifecycle event with an optional delay.
///
/// # Errors
///
/// Returns `sqlx::Error` on serialization failure or connection error.
pub async fn enqueue_ranking_event(
    pool: &PgPool,
    payload: RankingLifecyclePayload,
    delay_secs: i64,
) -> Result<i64, sqlx::Error> {
    let json_payload = serde_json::to_value(&payload)
        .map_err(|e| sqlx::Error::Protocol(format!("failed to serialize payload: {e}")))?;

    if delay_secs > 0 {
        #[allow(clippy::cast_possible_truncation)]
        let delay = delay_secs as i32;
        pgmq::send_delayed(pool, QUEUE_NAME, &json_payload, delay).await
    } else {
        pgmq::send(pool, QUEUE_NAME, &json_payload).await
    }
}

/// Read one ranking lifecycle message from the queue.
///
/// The message remains hidden from other consumers until the visibility timeout
/// elapses or it is archived.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn read_ranking_event(
    pool: &PgPool,
) -> Result<Option<RankingLifecycleMessage>, sqlx::Error> {
    let Some(msg) = pgmq::read(pool, QUEUE_NAME, VISIBILITY_TIMEOUT_SECS).await? else {
        return Ok(None);
    };

    let payload: RankingLifecyclePayload = serde_json::from_value(msg.message)
        .map_err(|e| sqlx::Error::Protocol(format!("invalid ranking lifecycle payload: {e}")))?;

    Ok(Some(RankingLifecycleMessage {
        msg_id: msg.msg_id,
        read_ct: msg.read_ct,
        payload,
        enqueued_at: msg.enqueued_at,
    }))
}

/// Archive a ranking lifecycle message after successful processing.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn archive_ranking_event(pool: &PgPool, msg_id: i64) -> Result<(), sqlx::Error> {
    pgmq::archive(pool, QUEUE_NAME, msg_id).await
}

/// Check if a message has exceeded the retry limit.
#[must_use]
pub const fn is_poison(msg: &RankingLifecycleMessage) -> bool {
    msg.read_ct > MAX_RETRIES
}

// ─── Consumer ────────────────────────────────────────────────────────────────

/// Spawn the ranking lifecycle consumer as a background tokio task.
///
/// Returns the [`tokio::task::JoinHandle`] so the caller can track or abort
/// the task on shutdown.
pub fn spawn_ranking_lifecycle_consumer(
    pool: PgPool,
    ranking_service: Arc<dyn RankingService>,
    room_lifecycle: Arc<dyn RoomLifecycle>,
    interval: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        tracing::info!(
            poll_interval_secs = interval.as_secs(),
            "ranking lifecycle consumer started"
        );
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            match read_ranking_event(&pool).await {
                Ok(Some(msg)) => {
                    if is_poison(&msg) {
                        tracing::warn!(
                            msg_id = msg.msg_id,
                            read_ct = msg.read_ct,
                            "archiving poison ranking lifecycle message"
                        );
                        if let Err(e) = archive_ranking_event(&pool, msg.msg_id).await {
                            tracing::warn!(
                                msg_id = msg.msg_id,
                                error = %e,
                                "failed to archive poison message"
                            );
                        }
                        continue;
                    }

                    tracing::debug!(msg_id = msg.msg_id, "processing ranking lifecycle event");
                    let success =
                        process_message(&pool, &*ranking_service, &*room_lifecycle, &msg).await;

                    if success {
                        if let Err(e) = archive_ranking_event(&pool, msg.msg_id).await {
                            tracing::warn!(
                                msg_id = msg.msg_id,
                                error = %e,
                                "failed to archive ranking lifecycle event"
                            );
                        }
                    }
                    // On failure: don't archive — VT expires and message redelivers.
                }
                Ok(None) => {} // Queue empty
                Err(e) => {
                    tracing::warn!("ranking lifecycle queue read failed: {e}");
                }
            }
        }
    })
}

/// Process a single lifecycle message. Returns `true` on success.
async fn process_message(
    pool: &PgPool,
    ranking_service: &dyn RankingService,
    room_lifecycle: &dyn RoomLifecycle,
    msg: &RankingLifecycleMessage,
) -> bool {
    match &msg.payload {
        RankingLifecyclePayload::OpenSubmit { room_id } => {
            handle_open_submit(pool, ranking_service, room_lifecycle, *room_id).await
        }
        RankingLifecyclePayload::OpenRanking { round_id, room_id } => {
            handle_open_ranking(pool, ranking_service, room_lifecycle, *round_id, *room_id).await
        }
        RankingLifecyclePayload::CloseRound { round_id, room_id } => {
            handle_close_round(pool, ranking_service, room_lifecycle, *round_id, *room_id).await
        }
    }
}

async fn handle_open_submit(
    pool: &PgPool,
    ranking_service: &dyn RankingService,
    room_lifecycle: &dyn RoomLifecycle,
    room_id: Uuid,
) -> bool {
    let room = match room_lifecycle.get_room(room_id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(room_id = %room_id, error = %e, "OpenSubmit: get_room failed");
            return false;
        }
    };

    let config: RankingConfig = match serde_json::from_value(room.engine_config.clone()) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                room_id = %room_id,
                error = %e,
                "OpenSubmit: failed to parse engine_config"
            );
            return false;
        }
    };

    let now = Utc::now();
    let submit_opens_at = now;
    let rank_opens_at = now + chrono::Duration::seconds(config.submit_duration_secs);
    let closes_at = rank_opens_at + chrono::Duration::seconds(config.rank_duration_secs);

    let round = match ranking_service
        .create_round(room_id, submit_opens_at, rank_opens_at, closes_at)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(
                room_id = %room_id,
                error = %e,
                "OpenSubmit: create_round failed"
            );
            return false;
        }
    };

    // Enqueue transition to ranking phase after submit phase ends.
    let delay = config.submit_duration_secs;
    if let Err(e) = enqueue_ranking_event(
        pool,
        RankingLifecyclePayload::OpenRanking {
            round_id: round.id,
            room_id,
        },
        delay,
    )
    .await
    {
        tracing::warn!(
            round_id = %round.id,
            error = %e,
            "OpenSubmit: failed to enqueue OpenRanking"
        );
        return false;
    }

    tracing::info!(
        room_id = %room_id,
        round_id = %round.id,
        delay_secs = delay,
        "round created; OpenRanking enqueued"
    );
    true
}

async fn handle_open_ranking(
    pool: &PgPool,
    ranking_service: &dyn RankingService,
    room_lifecycle: &dyn RoomLifecycle,
    round_id: Uuid,
    room_id: Uuid,
) -> bool {
    if let Err(e) = ranking_service.open_ranking(round_id).await {
        tracing::warn!(
            round_id = %round_id,
            error = %e,
            "OpenRanking: open_ranking failed"
        );
        return false;
    }

    let room = match room_lifecycle.get_room(room_id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(
                room_id = %room_id,
                error = %e,
                "OpenRanking: get_room failed"
            );
            return false;
        }
    };

    let config: RankingConfig = match serde_json::from_value(room.engine_config.clone()) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                room_id = %room_id,
                error = %e,
                "OpenRanking: failed to parse engine_config"
            );
            return false;
        }
    };

    let delay = config.rank_duration_secs;
    if let Err(e) = enqueue_ranking_event(
        pool,
        RankingLifecyclePayload::CloseRound { round_id, room_id },
        delay,
    )
    .await
    {
        tracing::warn!(
            round_id = %round_id,
            error = %e,
            "OpenRanking: failed to enqueue CloseRound"
        );
        return false;
    }

    tracing::info!(
        round_id = %round_id,
        delay_secs = delay,
        "ranking opened; CloseRound enqueued"
    );
    true
}

async fn handle_close_round(
    pool: &PgPool,
    ranking_service: &dyn RankingService,
    room_lifecycle: &dyn RoomLifecycle,
    round_id: Uuid,
    room_id: Uuid,
) -> bool {
    let room = match room_lifecycle.get_room(room_id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(
                room_id = %room_id,
                error = %e,
                "CloseRound: get_room failed"
            );
            return false;
        }
    };

    let config: RankingConfig = match serde_json::from_value(room.engine_config.clone()) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                room_id = %room_id,
                error = %e,
                "CloseRound: failed to parse engine_config"
            );
            return false;
        }
    };

    if let Err(e) = ranking_service
        .close_round(round_id, config.hall_of_fame_depth)
        .await
    {
        tracing::warn!(
            round_id = %round_id,
            error = %e,
            "CloseRound: close_round failed"
        );
        return false;
    }

    // Immediately kick off the next cycle.
    if let Err(e) =
        enqueue_ranking_event(pool, RankingLifecyclePayload::OpenSubmit { room_id }, 0).await
    {
        tracing::warn!(
            room_id = %room_id,
            error = %e,
            "CloseRound: failed to enqueue next OpenSubmit"
        );
        return false;
    }

    tracing::info!(
        round_id = %round_id,
        room_id = %room_id,
        "round closed; next OpenSubmit enqueued"
    );
    true
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_serializes_with_type_tag() {
        let p = RankingLifecyclePayload::OpenSubmit {
            room_id: Uuid::nil(),
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["type"], "open_submit");
        assert!(v.get("room_id").is_some());
    }

    #[test]
    fn payload_roundtrips_open_ranking() {
        let p = RankingLifecyclePayload::OpenRanking {
            round_id: Uuid::nil(),
            room_id: Uuid::nil(),
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["type"], "open_ranking");
        let back: RankingLifecyclePayload = serde_json::from_value(v).unwrap();
        assert!(matches!(back, RankingLifecyclePayload::OpenRanking { .. }));
    }

    #[test]
    fn payload_roundtrips_close_round() {
        let p = RankingLifecyclePayload::CloseRound {
            round_id: Uuid::nil(),
            room_id: Uuid::nil(),
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["type"], "close_round");
        let back: RankingLifecyclePayload = serde_json::from_value(v).unwrap();
        assert!(matches!(back, RankingLifecyclePayload::CloseRound { .. }));
    }

    #[test]
    fn is_poison_detects_excess_reads() {
        let make_msg = |read_ct: i32| RankingLifecycleMessage {
            msg_id: 1,
            read_ct,
            payload: RankingLifecyclePayload::OpenSubmit {
                room_id: Uuid::nil(),
            },
            enqueued_at: Utc::now(),
        };
        assert!(!is_poison(&make_msg(1)));
        assert!(!is_poison(&make_msg(3)));
        assert!(is_poison(&make_msg(4)));
        assert!(is_poison(&make_msg(10)));
    }
}

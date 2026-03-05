//! Background consumer for the lifecycle message queue.

use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;

use super::repo::lifecycle_queue::{read_lifecycle_event, LifecyclePayload};
use super::service::RoomsService;

/// Spawn the lifecycle consumer as a background tokio task.
pub fn spawn_lifecycle_consumer(
    pool: PgPool,
    service: Arc<dyn RoomsService>,
    poll_interval: Duration,
) {
    tokio::spawn(async move {
        tracing::info!(
            poll_interval_ms = poll_interval.as_secs(),
            "lifecycle consumer started"
        );
        let mut interval = tokio::time::interval(poll_interval);
        loop {
            interval.tick().await;
            match read_lifecycle_event(&pool).await {
                Ok(Some(msg)) => {
                    tracing::debug!(msg_id = msg.id, "processing lifecycle event");
                    match msg.payload {
                        LifecyclePayload::ClosePoll { poll_id, room_id } => {
                            if let Err(e) = service.close_poll_and_advance(room_id, poll_id).await {
                                tracing::warn!(
                                    poll_id = %poll_id,
                                    room_id = %room_id,
                                    error = %e,
                                    "close_poll_and_advance failed"
                                );
                            }
                        }
                        LifecyclePayload::ActivateNext { room_id } => {
                            if let Err(e) = service.activate_next_from_agenda(room_id).await {
                                tracing::warn!(
                                    room_id = %room_id,
                                    error = %e,
                                    "activate_next_from_agenda failed"
                                );
                            }
                        }
                    }
                }
                Ok(None) => {} // Queue empty
                Err(e) => {
                    tracing::warn!("lifecycle queue read failed: {e}");
                }
            }
        }
    });
}

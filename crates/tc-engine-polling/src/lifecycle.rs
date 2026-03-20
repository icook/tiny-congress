//! Background consumer for the lifecycle message queue.

use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;

use crate::repo::lifecycle_queue::{
    archive_lifecycle_event, is_poison, read_lifecycle_event, LifecyclePayload,
};
use crate::service::PollingService;

/// Spawn the lifecycle consumer as a background tokio task.
///
/// Returns the [`tokio::task::JoinHandle`] so the caller can track or
/// abort the task on shutdown.
pub fn spawn_lifecycle_consumer(
    pool: PgPool,
    polling_service: Arc<dyn PollingService>,
    poll_interval: Duration,
) -> tokio::task::JoinHandle<()> {
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
                    if is_poison(&msg) {
                        tracing::warn!(
                            msg_id = msg.msg_id,
                            read_ct = msg.read_ct,
                            "archiving poison lifecycle message"
                        );
                        if let Err(e) = archive_lifecycle_event(&pool, msg.msg_id).await {
                            tracing::warn!(msg_id = msg.msg_id, error = %e, "failed to archive poison message");
                        }
                        continue;
                    }

                    tracing::debug!(msg_id = msg.msg_id, "processing lifecycle event");
                    let success = match msg.payload {
                        LifecyclePayload::ClosePoll { poll_id, room_id } => polling_service
                            .close_poll_and_advance(room_id, poll_id)
                            .await
                            .map_err(|e| {
                                tracing::warn!(
                                    poll_id = %poll_id,
                                    room_id = %room_id,
                                    error = %e,
                                    "close_poll_and_advance failed"
                                );
                            })
                            .is_ok(),
                        LifecyclePayload::ActivateNext { room_id } => polling_service
                            .activate_next_from_agenda(room_id)
                            .await
                            .map_err(|e| {
                                tracing::warn!(
                                    room_id = %room_id,
                                    error = %e,
                                    "activate_next_from_agenda failed"
                                );
                            })
                            .is_ok(),
                    };

                    if success {
                        if let Err(e) = archive_lifecycle_event(&pool, msg.msg_id).await {
                            tracing::warn!(msg_id = msg.msg_id, error = %e, "failed to archive lifecycle event");
                        }
                    }
                    // On failure: don't archive — VT will expire and message redelivers
                }
                Ok(None) => {} // Queue empty
                Err(e) => {
                    tracing::warn!("lifecycle queue read failed: {e}");
                }
            }
        }
    })
}

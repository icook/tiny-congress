//! Background worker that drains the bot task queue.
//!
//! Reads one message at a time from the pgmq `rooms__bot_tasks` queue,
//! dispatches it to the appropriate handler, then archives or requeues it.

use std::time::Duration;

use sqlx::PgPool;
use uuid::Uuid;

use crate::repo::{bot_traces, pgmq};

/// Maximum number of delivery attempts before a message is treated as poison.
const MAX_RETRIES: i32 = 3;

/// Visibility timeout: how long a message is hidden from other consumers while
/// being processed (seconds).
const VISIBILITY_TIMEOUT_SECS: i32 = 300; // 5 minutes

/// How long to sleep when the queue is empty before polling again.
const POLL_INTERVAL: Duration = Duration::from_secs(5);

// ─── Public entry point ──────────────────────────────────────────────────────

/// Spawn the bot worker as a background tokio task.
///
/// Returns the [`tokio::task::JoinHandle`] so the caller can track or
/// abort the task on shutdown.
#[must_use]
pub fn spawn_bot_worker(pool: PgPool) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        tracing::info!("bot worker started");
        loop {
            match pgmq::read_task(&pool, VISIBILITY_TIMEOUT_SECS).await {
                Ok(Some(msg)) => {
                    if msg.read_ct > MAX_RETRIES {
                        tracing::warn!(
                            msg_id = msg.msg_id,
                            read_ct = msg.read_ct,
                            "archiving poison message after {} retries",
                            MAX_RETRIES
                        );
                        if let Err(e) = pgmq::archive_task(&pool, msg.msg_id).await {
                            tracing::warn!(msg_id = msg.msg_id, error = %e, "failed to archive poison message");
                        }
                        continue;
                    }

                    let task: pgmq::BotTask = match serde_json::from_value(msg.message.clone()) {
                        Ok(t) => t,
                        Err(e) => {
                            tracing::warn!(
                                msg_id = msg.msg_id,
                                error = %e,
                                "failed to deserialize BotTask; archiving malformed message"
                            );
                            if let Err(ae) = pgmq::archive_task(&pool, msg.msg_id).await {
                                tracing::warn!(msg_id = msg.msg_id, error = %ae, "failed to archive malformed message");
                            }
                            continue;
                        }
                    };

                    match execute_task(&pool, &task).await {
                        Ok(()) => {
                            if let Err(e) = pgmq::archive_task(&pool, msg.msg_id).await {
                                tracing::warn!(
                                    msg_id = msg.msg_id,
                                    error = %e,
                                    "failed to archive completed task"
                                );
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                msg_id = msg.msg_id,
                                task = %task.task,
                                room_id = %task.room_id,
                                error = %e,
                                "bot task failed; visibility timeout will handle redelivery"
                            );
                        }
                    }
                }
                Ok(None) => {
                    // Queue empty — sleep before next poll
                    tokio::time::sleep(POLL_INTERVAL).await;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "bot task queue read failed");
                    tokio::time::sleep(POLL_INTERVAL).await;
                }
            }
        }
    })
}

// ─── Task dispatch ───────────────────────────────────────────────────────────

/// Execute a single bot task, wrapping it in a trace record.
async fn execute_task(pool: &PgPool, task: &pgmq::BotTask) -> anyhow::Result<()> {
    let trace_id: Uuid =
        bot_traces::create_trace(pool, task.room_id, &task.task, "iterate").await?;

    let result = dispatch_task(pool, task);

    match &result {
        Ok(()) => {
            bot_traces::complete_trace(pool, trace_id, None).await?;
        }
        Err(e) => {
            bot_traces::fail_trace(pool, trace_id, &e.to_string()).await?;
        }
    }

    result
}

/// Dispatch to the concrete handler for each task variant.
fn dispatch_task(_pool: &PgPool, task: &pgmq::BotTask) -> anyhow::Result<()> {
    match task.task.as_str() {
        "research_company" => {
            tracing::info!(room_id = %task.room_id, "stub: research_company");
            Ok(())
        }
        "generate_evidence" => {
            tracing::info!(room_id = %task.room_id, "stub: generate_evidence");
            Ok(())
        }
        other => {
            anyhow::bail!("unknown bot task type: {other:?}")
        }
    }
}

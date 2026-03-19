//! Bot trace persistence operations
//!
//! Stores LLM/search execution traces for room bots, including step-by-step
//! costs, latency, and output summaries.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

// ─── Types ──────────────────────────────────────────────────────────────────

/// A single step within a bot trace (one LLM call or Exa search).
#[derive(Debug, Serialize, Deserialize)]
pub struct TraceStep {
    #[serde(rename = "type")]
    pub step_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<u32>,
    pub latency_ms: u64,
    pub cost_usd: f64,
    pub cache: serde_json::Value,
    pub output_summary: String,
}

/// A full bot trace record as stored in the database.
#[derive(Debug, sqlx::FromRow)]
pub struct BotTrace {
    pub id: Uuid,
    pub room_id: Uuid,
    pub poll_id: Option<Uuid>,
    pub task: String,
    pub run_mode: String,
    pub steps: serde_json::Value,
    pub total_cost_usd: f64,
    pub status: String,
    pub error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

// ─── Operations ─────────────────────────────────────────────────────────────

/// Insert a new trace in `running` status and return its ID.
///
/// # Errors
///
/// Returns a database error on connection failure.
pub async fn create_trace(
    pool: &PgPool,
    room_id: Uuid,
    task: &str,
    run_mode: &str,
) -> Result<Uuid> {
    let row: (Uuid,) = sqlx::query_as(
        r"
        INSERT INTO rooms__bot_traces (room_id, task, run_mode)
        VALUES ($1, $2, $3)
        RETURNING id
        ",
    )
    .bind(room_id)
    .bind(task)
    .bind(run_mode)
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}

/// Append one step to the trace's `steps` array and add its cost to the total.
///
/// # Errors
///
/// Returns a database error on connection failure or if the trace does not exist.
pub async fn append_step(pool: &PgPool, trace_id: Uuid, step: &TraceStep) -> Result<()> {
    let step_json = serde_json::to_value(step)?;
    sqlx::query(
        r"
        UPDATE rooms__bot_traces
        SET steps = steps || $1::jsonb,
            total_cost_usd = total_cost_usd + $2
        WHERE id = $3
        ",
    )
    .bind(step_json)
    .bind(step.cost_usd)
    .bind(trace_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Mark a trace as completed and optionally link it to a poll.
///
/// # Errors
///
/// Returns a database error on connection failure.
pub async fn complete_trace(pool: &PgPool, trace_id: Uuid, poll_id: Option<Uuid>) -> Result<()> {
    sqlx::query(
        r"
        UPDATE rooms__bot_traces
        SET status = 'completed',
            poll_id = COALESCE($1, poll_id),
            completed_at = now()
        WHERE id = $2
        ",
    )
    .bind(poll_id)
    .bind(trace_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Mark a trace as failed with an error message.
///
/// # Errors
///
/// Returns a database error on connection failure.
pub async fn fail_trace(pool: &PgPool, trace_id: Uuid, error: &str) -> Result<()> {
    sqlx::query(
        r"
        UPDATE rooms__bot_traces
        SET status = 'failed',
            error = $1,
            completed_at = now()
        WHERE id = $2
        ",
    )
    .bind(error)
    .bind(trace_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Fetch all traces associated with a poll, newest first.
///
/// # Errors
///
/// Returns a database error on connection failure.
pub async fn get_traces_for_poll(pool: &PgPool, poll_id: Uuid) -> Result<Vec<BotTrace>> {
    let rows = sqlx::query_as::<_, BotTrace>(
        r"
        SELECT id, room_id, poll_id, task, run_mode, steps,
               total_cost_usd::float8, status, error, created_at, completed_at
        FROM rooms__bot_traces
        WHERE poll_id = $1
        ORDER BY created_at DESC
        ",
    )
    .bind(poll_id)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Fetch the most recent traces for a room.
///
/// # Errors
///
/// Returns a database error on connection failure.
pub async fn get_traces_for_room(
    pool: &PgPool,
    room_id: Uuid,
    limit: i64,
) -> Result<Vec<BotTrace>> {
    let rows = sqlx::query_as::<_, BotTrace>(
        r"
        SELECT id, room_id, poll_id, task, run_mode, steps,
               total_cost_usd::float8, status, error, created_at, completed_at
        FROM rooms__bot_traces
        WHERE room_id = $1
        ORDER BY created_at DESC
        LIMIT $2
        ",
    )
    .bind(room_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

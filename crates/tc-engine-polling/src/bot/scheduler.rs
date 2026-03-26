//! Scheduler loop — scans bot-enabled rooms and enqueues research tasks
//! to maintain a buffer of draft polls.

use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

use crate::bot::config::{BotConfig, CompanyTopic};
use crate::repo::pgmq::{self, BotTask};

const BUFFER_SIZE: usize = 5;
const DEFAULT_SCHEDULE_SECS: u64 = 60;

/// Room row with only the fields the scheduler needs.
#[derive(sqlx::FromRow)]
struct SchedulerRoom {
    id: Uuid,
    engine_config: serde_json::Value,
}

/// Spawn the scheduler as a background tokio task.
#[must_use]
pub fn spawn_scheduler(pool: PgPool) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if let Err(e) = tick(&pool).await {
                tracing::error!("bot scheduler tick failed: {e}");
            }
            tokio::time::sleep(Duration::from_secs(DEFAULT_SCHEDULE_SECS)).await;
        }
    })
}

/// One scheduler tick: scan all bot-enabled rooms and enqueue tasks as needed.
///
/// # Errors
///
/// Returns an error if the database query fails.
pub async fn tick(pool: &PgPool) -> anyhow::Result<()> {
    let rooms: Vec<SchedulerRoom> = sqlx::query_as(
        "SELECT id, engine_config FROM rooms__rooms \
         WHERE status = 'open' \
           AND engine_config->'bot'->>'enabled' = 'true'",
    )
    .fetch_all(pool)
    .await?;

    for room in rooms {
        if let Err(e) = fill_room(pool, &room).await {
            tracing::error!(room_id = %room.id, "scheduler: fill_room failed: {e}");
        }
    }

    Ok(())
}

/// Check a single room's draft buffer and enqueue research tasks to fill it.
async fn fill_room(pool: &PgPool, room: &SchedulerRoom) -> anyhow::Result<()> {
    let Some(config) = BotConfig::from_engine_config(&room.engine_config) else {
        return Ok(());
    };

    if config.topics.is_empty() {
        return Ok(());
    }

    if config.topic_cursor >= config.topics.len() {
        tracing::debug!(room_id = %room.id, "scheduler: topic list exhausted");
        return Ok(());
    }

    let draft_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM rooms__polls WHERE room_id = $1 AND status = 'draft'",
    )
    .bind(room.id)
    .fetch_one(pool)
    .await?;

    let (topics_to_enqueue, new_cursor) = compute_deficit(
        &config.topics,
        config.topic_cursor,
        usize::try_from(draft_count).unwrap_or(usize::MAX),
        BUFFER_SIZE,
    );

    if topics_to_enqueue.is_empty() {
        return Ok(());
    }

    let enqueued_count = topics_to_enqueue.len();

    for topic in topics_to_enqueue {
        let params = serde_json::json!({
            "company": topic.company,
            "ticker": topic.ticker.as_deref().unwrap_or(""),
        });

        pgmq::send_task(
            pool,
            &BotTask {
                room_id: room.id,
                task: "research_company".to_string(),
                params,
            },
        )
        .await?;
    }

    // Persist the updated cursor
    sqlx::query(
        "UPDATE rooms__rooms \
         SET engine_config = jsonb_set(engine_config, '{bot,topic_cursor}', $1::jsonb) \
         WHERE id = $2",
    )
    .bind(serde_json::json!(new_cursor))
    .bind(room.id)
    .execute(pool)
    .await?;

    tracing::info!(
        room_id = %room.id,
        enqueued = enqueued_count,
        cursor = new_cursor,
        "scheduler: enqueued research tasks"
    );

    Ok(())
}

/// Compute how many tasks to enqueue and the new cursor position.
///
/// Returns a slice of topics to enqueue and the resulting cursor value.
fn compute_deficit(
    topics: &[CompanyTopic],
    cursor: usize,
    draft_count: usize,
    buffer_size: usize,
) -> (&[CompanyTopic], usize) {
    let deficit = buffer_size.saturating_sub(draft_count);
    let available = topics.len().saturating_sub(cursor);
    let count = deficit.min(available);
    (&topics[cursor..cursor + count], cursor + count)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::config::CompanyTopic;

    fn make_topics(names: &[&str]) -> Vec<CompanyTopic> {
        names
            .iter()
            .map(|&n| CompanyTopic {
                company: n.to_string(),
                ticker: None,
            })
            .collect()
    }

    #[test]
    fn compute_deficit_fills_empty_buffer() {
        let topics = make_topics(&["A", "B", "C", "D", "E", "F"]);
        let (slice, new_cursor) = compute_deficit(&topics, 0, 0, 5);
        assert_eq!(slice.len(), 5);
        assert_eq!(new_cursor, 5);
    }

    #[test]
    fn compute_deficit_respects_existing_drafts() {
        let topics = make_topics(&["A", "B", "C", "D", "E", "F"]);
        let (slice, new_cursor) = compute_deficit(&topics, 0, 3, 5);
        assert_eq!(slice.len(), 2);
        assert_eq!(new_cursor, 2);
    }

    #[test]
    fn compute_deficit_buffer_full_returns_empty() {
        let topics = make_topics(&["A", "B", "C", "D", "E"]);
        let (slice, new_cursor) = compute_deficit(&topics, 0, 5, 5);
        assert!(slice.is_empty());
        assert_eq!(new_cursor, 0);
    }

    #[test]
    fn compute_deficit_stops_at_end_of_topics() {
        let topics = make_topics(&["A", "B"]);
        let (slice, new_cursor) = compute_deficit(&topics, 1, 0, 5);
        assert_eq!(slice.len(), 1);
        assert_eq!(new_cursor, 2);
    }

    #[test]
    fn compute_deficit_cursor_at_end_returns_empty() {
        let topics = make_topics(&["A", "B"]);
        let (slice, new_cursor) = compute_deficit(&topics, 2, 0, 5);
        assert!(slice.is_empty());
        assert_eq!(new_cursor, 2);
    }
}

//! Integration tests for the bot scheduler tick.

mod common;

use common::test_db::isolated_db;
use serde_json::json;
use tc_engine_polling::bot::scheduler::tick;
use tc_engine_polling::repo::pgmq;
use tc_test_macros::shared_runtime_test;

// ---------------------------------------------------------------------------
// Test 1: tick enqueues research tasks and advances the topic_cursor
// ---------------------------------------------------------------------------

/// A room with 3 topics and no draft polls should have `tick` enqueue 3
/// research_company tasks (buffer_size=5 > 3 available) and advance
/// `topic_cursor` from 0 to 3.
#[shared_runtime_test]
async fn test_tick_enqueues_tasks_and_advances_cursor() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    // Insert a bot-enabled room with 3 topics and cursor at 0.
    let engine_config = json!({
        "bot": {
            "enabled": true,
            "topics": [
                { "company": "Apple",   "ticker": "AAPL" },
                { "company": "Google",  "ticker": "GOOGL" },
                { "company": "Amazon",  "ticker": "AMZN" }
            ],
            "topic_cursor": 0
        }
    });

    let room_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO rooms__rooms (name, status, engine_config) \
         VALUES ($1, 'open', $2) RETURNING id",
    )
    .bind("bot-test-room")
    .bind(&engine_config)
    .fetch_one(&pool)
    .await
    .expect("insert room");

    // Run the scheduler tick.
    tick(&pool).await.expect("tick should not fail");

    // Assert: 3 messages were enqueued in pgmq.
    let queue_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pgmq.q_rooms__bot_tasks")
        .fetch_one(&pool)
        .await
        .expect("count queue messages");
    assert_eq!(queue_count, 3, "expected 3 messages enqueued for 3 topics");

    // Assert: topic_cursor was persisted to 3.
    let cursor: i64 = sqlx::query_scalar(
        "SELECT (engine_config->'bot'->>'topic_cursor')::bigint \
         FROM rooms__rooms WHERE id = $1",
    )
    .bind(room_id)
    .fetch_one(&pool)
    .await
    .expect("read topic_cursor");
    assert_eq!(cursor, 3, "topic_cursor should be advanced to 3");

    // Assert: each message has the expected task type and correct room_id.
    let messages = read_all_bot_tasks(&pool).await;
    assert_eq!(messages.len(), 3, "expected 3 readable messages");
    for msg in &messages {
        assert_eq!(
            msg["room_id"].as_str().unwrap(),
            room_id.to_string(),
            "all tasks should reference the bot room"
        );
        assert_eq!(
            msg["task"].as_str().unwrap(),
            "research_company",
            "all tasks should be research_company"
        );
    }

    // Assert: companies in enqueued tasks match the topics list (order may vary).
    let companies: Vec<&str> = messages
        .iter()
        .map(|m| m["params"]["company"].as_str().unwrap())
        .collect();
    assert!(companies.contains(&"Apple"), "Apple should be enqueued");
    assert!(companies.contains(&"Google"), "Google should be enqueued");
    assert!(companies.contains(&"Amazon"), "Amazon should be enqueued");
}

// ---------------------------------------------------------------------------
// Test 2: tick skips rooms with bot disabled
// ---------------------------------------------------------------------------

/// A room where `bot.enabled = false` should produce no pgmq messages.
#[shared_runtime_test]
async fn test_tick_skips_disabled_bot_room() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let engine_config = json!({
        "bot": {
            "enabled": false,
            "topics": [
                { "company": "Apple", "ticker": "AAPL" }
            ]
        }
    });

    sqlx::query(
        "INSERT INTO rooms__rooms (name, status, engine_config) \
         VALUES ($1, 'open', $2)",
    )
    .bind("disabled-bot-room")
    .bind(&engine_config)
    .execute(&pool)
    .await
    .expect("insert room");

    tick(&pool).await.expect("tick should not fail");

    let queue_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pgmq.q_rooms__bot_tasks")
        .fetch_one(&pool)
        .await
        .expect("count queue messages");
    assert_eq!(queue_count, 0, "disabled bot should produce no messages");
}

// ---------------------------------------------------------------------------
// Test 3: tick does not enqueue when draft buffer is already full
// ---------------------------------------------------------------------------

/// When a room already has 5 draft polls (= BUFFER_SIZE), tick should not
/// enqueue any tasks even if topics remain.
#[shared_runtime_test]
async fn test_tick_skips_when_buffer_full() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let engine_config = json!({
        "bot": {
            "enabled": true,
            "topics": [
                { "company": "Apple" },
                { "company": "Google" },
                { "company": "Amazon" }
            ],
            "topic_cursor": 0
        }
    });

    let room_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO rooms__rooms (name, status, engine_config) \
         VALUES ($1, 'open', $2) RETURNING id",
    )
    .bind("full-buffer-room")
    .bind(&engine_config)
    .fetch_one(&pool)
    .await
    .expect("insert room");

    // Seed 5 draft polls to fill the buffer.
    for i in 0..5i32 {
        sqlx::query(
            "INSERT INTO rooms__polls (room_id, question, status) \
             VALUES ($1, $2, 'draft')",
        )
        .bind(room_id)
        .bind(format!("Draft question {i}"))
        .execute(&pool)
        .await
        .expect("insert draft poll");
    }

    tick(&pool).await.expect("tick should not fail");

    let queue_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pgmq.q_rooms__bot_tasks")
        .fetch_one(&pool)
        .await
        .expect("count queue messages");
    assert_eq!(queue_count, 0, "full buffer should suppress enqueue");

    // Cursor should remain at 0 (no tasks enqueued, no cursor advance).
    let cursor: i64 = sqlx::query_scalar(
        "SELECT (engine_config->'bot'->>'topic_cursor')::bigint \
         FROM rooms__rooms WHERE id = $1",
    )
    .bind(room_id)
    .fetch_one(&pool)
    .await
    .expect("read topic_cursor");
    assert_eq!(cursor, 0, "cursor should not advance when buffer is full");
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Drain all visible messages from `rooms__bot_tasks` and return their
/// JSON payloads.  Uses a visibility timeout of 1 second per read so all
/// messages can be consumed in a tight loop.
async fn read_all_bot_tasks(pool: &sqlx::PgPool) -> Vec<serde_json::Value> {
    let mut results = Vec::new();
    loop {
        match pgmq::read_task(pool, 1).await.expect("read_task") {
            Some(msg) => results.push(msg.message),
            None => break,
        }
    }
    results
}

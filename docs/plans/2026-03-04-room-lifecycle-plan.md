# Room Lifecycle Engine Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a pgmq-driven lifecycle engine that auto-rotates polls within long-lived rooms on a configurable cadence.

**Architecture:** Rooms gain a `poll_duration_secs` config. Polls gain `closes_at` and `agenda_position`. A lightweight lifecycle queue table (plain SQL, no pgmq extension) provides delayed message delivery. A background tokio task in the API server consumes lifecycle events to close expired polls and activate the next one from the room's agenda. The sim worker becomes a pure content creator that checks capacity and fills agenda slots.

**Tech Stack:** Rust (axum, sqlx, tokio), PostgreSQL (FOR UPDATE SKIP LOCKED for atomic queue reads), existing testcontainers test infrastructure.

**Design doc:** `docs/plans/2026-03-04-room-lifecycle-engine-design.md`

---

### Task 1: Database Migration

**Files:**
- Create: `service/migrations/11_room_lifecycle.sql`

**Step 1: Write the migration**

```sql
-- Room lifecycle: cadence config + poll scheduling + message queue

-- Room-level rotation cadence (NULL = manual-only, no auto-rotation)
ALTER TABLE rooms__rooms ADD COLUMN poll_duration_secs INTEGER;

-- Poll scheduling within a room's agenda
ALTER TABLE rooms__polls ADD COLUMN closes_at TIMESTAMPTZ;
ALTER TABLE rooms__polls ADD COLUMN agenda_position INTEGER;

-- Lightweight lifecycle message queue (pgmq semantics via plain SQL)
-- Messages become visible at visible_at; consumed via FOR UPDATE SKIP LOCKED.
CREATE TABLE rooms__lifecycle_queue (
    id BIGSERIAL PRIMARY KEY,
    message_type TEXT NOT NULL CHECK (message_type IN ('close_poll', 'activate_next')),
    payload JSONB NOT NULL,
    visible_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_lifecycle_queue_visible
    ON rooms__lifecycle_queue (visible_at, id)
    WHERE visible_at <= now();
```

**Step 2: Verify migration applies**

Run: `just test-backend` (migrations run automatically via testcontainers)
Expected: All existing tests still pass — new columns are nullable, new table is additive.

**Step 3: Commit**

```bash
git add service/migrations/11_room_lifecycle.sql
git commit -m "feat(rooms): add lifecycle migration — cadence, agenda, queue table"
```

---

### Task 2: Queue Repo Layer

**Files:**
- Create: `service/src/rooms/repo/lifecycle_queue.rs`
- Modify: `service/src/rooms/repo/mod.rs` (add `pub mod lifecycle_queue` and re-export)

**Step 1: Write tests for queue operations**

Add to `service/tests/rooms_handler_tests.rs` (or a new `service/tests/lifecycle_queue_tests.rs`):

```rust
// service/tests/lifecycle_queue_tests.rs
mod common;

use common::test_db::isolated_db;
use tc_test_macros::shared_runtime_test;
use tinycongress_api::rooms::repo::lifecycle_queue::{
    enqueue_lifecycle_event, read_lifecycle_event, delete_lifecycle_event,
    LifecycleMessage, LifecyclePayload,
};

#[shared_runtime_test]
async fn test_enqueue_and_read_immediate_message() {
    let pool = isolated_db().await;
    let payload = LifecyclePayload::ActivateNext {
        room_id: uuid::Uuid::new_v4(),
    };
    let msg_id = enqueue_lifecycle_event(&pool, &payload, 0).await.unwrap();
    assert!(msg_id > 0);

    let msg = read_lifecycle_event(&pool).await.unwrap();
    assert!(msg.is_some());
    let msg = msg.unwrap();
    assert_eq!(msg.id, msg_id);

    delete_lifecycle_event(&pool, msg.id).await.unwrap();

    // Queue should be empty now
    let msg = read_lifecycle_event(&pool).await.unwrap();
    assert!(msg.is_none());
}

#[shared_runtime_test]
async fn test_delayed_message_not_visible_yet() {
    let pool = isolated_db().await;
    let payload = LifecyclePayload::ClosePoll {
        poll_id: uuid::Uuid::new_v4(),
        room_id: uuid::Uuid::new_v4(),
    };
    // Delay 3600 seconds — should not be visible
    enqueue_lifecycle_event(&pool, &payload, 3600).await.unwrap();

    let msg = read_lifecycle_event(&pool).await.unwrap();
    assert!(msg.is_none(), "delayed message should not be visible yet");
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p tinycongress-api --test lifecycle_queue_tests`
Expected: Compilation error (module doesn't exist yet)

**Step 3: Implement the queue repo**

```rust
// service/src/rooms/repo/lifecycle_queue.rs
//! Lightweight lifecycle message queue backed by a Postgres table.
//!
//! Provides pgmq-like semantics (delayed visibility, atomic consumption)
//! without requiring the pgmq extension. Messages are consumed via
//! `FOR UPDATE SKIP LOCKED` for safe concurrent access.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum LifecyclePayload {
    #[serde(rename = "close_poll")]
    ClosePoll { poll_id: Uuid, room_id: Uuid },
    #[serde(rename = "activate_next")]
    ActivateNext { room_id: Uuid },
}

impl LifecyclePayload {
    fn message_type(&self) -> &'static str {
        match self {
            Self::ClosePoll { .. } => "close_poll",
            Self::ActivateNext { .. } => "activate_next",
        }
    }
}

#[derive(Debug)]
pub struct LifecycleMessage {
    pub id: i64,
    pub payload: LifecyclePayload,
    pub created_at: DateTime<Utc>,
}

/// Enqueue a lifecycle event with an optional delay in seconds.
///
/// # Errors
/// Returns a database error on failure.
pub async fn enqueue_lifecycle_event<'e, E>(
    executor: E,
    payload: &LifecyclePayload,
    delay_secs: i64,
) -> Result<i64, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let json = serde_json::to_value(payload)
        .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
    let row: (i64,) = sqlx::query_as(
        r"
        INSERT INTO rooms__lifecycle_queue (message_type, payload, visible_at)
        VALUES ($1, $2, now() + make_interval(secs => $3::double precision))
        RETURNING id
        ",
    )
    .bind(payload.message_type())
    .bind(&json)
    .bind(delay_secs as f64)
    .fetch_one(executor)
    .await?;
    Ok(row.0)
}

/// Atomically read and lock the next visible message.
///
/// Uses `FOR UPDATE SKIP LOCKED` so multiple consumers can safely
/// compete for messages without blocking each other.
///
/// # Errors
/// Returns a database error on failure.
pub async fn read_lifecycle_event(
    pool: &sqlx::PgPool,
) -> Result<Option<LifecycleMessage>, sqlx::Error> {
    // Must use a transaction for FOR UPDATE SKIP LOCKED to work
    let mut tx = pool.begin().await?;

    let row: Option<(i64, serde_json::Value, DateTime<Utc>)> = sqlx::query_as(
        r"
        SELECT id, payload, created_at
        FROM rooms__lifecycle_queue
        WHERE visible_at <= now()
        ORDER BY id
        LIMIT 1
        FOR UPDATE SKIP LOCKED
        ",
    )
    .fetch_optional(&mut *tx)
    .await?;

    let Some((id, json, created_at)) = row else {
        tx.commit().await?;
        return Ok(None);
    };

    // Delete the message within the same transaction (pop semantics)
    sqlx::query("DELETE FROM rooms__lifecycle_queue WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let payload: LifecyclePayload = serde_json::from_value(json)
        .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;

    Ok(Some(LifecycleMessage {
        id,
        payload,
        created_at,
    }))
}

/// Delete a message by ID (for manual acknowledgment if not using pop semantics).
///
/// # Errors
/// Returns a database error on failure.
pub async fn delete_lifecycle_event<'e, E>(
    executor: E,
    message_id: i64,
) -> Result<(), sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query("DELETE FROM rooms__lifecycle_queue WHERE id = $1")
        .bind(message_id)
        .execute(executor)
        .await?;
    Ok(())
}
```

**Step 4: Register the module in `mod.rs`**

Add to `service/src/rooms/repo/mod.rs` after line 4 (`pub mod votes;`):

```rust
pub mod lifecycle_queue;
```

And add re-export after line 9:

```rust
pub use lifecycle_queue::{
    enqueue_lifecycle_event, read_lifecycle_event, LifecycleMessage, LifecyclePayload,
};
```

**Step 5: Run tests**

Run: `cargo test -p tinycongress-api --test lifecycle_queue_tests`
Expected: PASS

**Step 6: Commit**

```bash
git add service/src/rooms/repo/lifecycle_queue.rs service/src/rooms/repo/mod.rs \
    service/tests/lifecycle_queue_tests.rs
git commit -m "feat(rooms): add lifecycle queue repo with pop semantics"
```

---

### Task 3: Extend Room and Poll Records

**Files:**
- Modify: `service/src/rooms/repo/rooms.rs` (add `poll_duration_secs` to record + row + queries)
- Modify: `service/src/rooms/repo/polls.rs` (add `closes_at`, `agenda_position` to record + row + queries)

**Step 1: Write test for creating a room with cadence**

In `service/tests/rooms_handler_tests.rs`, add:

```rust
#[shared_runtime_test]
async fn test_create_room_with_poll_duration() {
    let pool = isolated_db().await;
    let (app, keys, _account_id) = signup_and_get_account("cadence_user", &pool).await;
    let body = serde_json::json!({
        "name": "Cadence Room",
        "description": "A room with auto-rotation",
        "poll_duration_secs": 3600
    });
    let response = app
        .clone()
        .oneshot(build_authed_request(
            &keys,
            Method::POST,
            "/rooms",
            Some(body.to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = parse_body(response).await;
    assert_eq!(body["poll_duration_secs"], 3600);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p tinycongress-api --test rooms_handler_tests test_create_room_with_poll_duration`
Expected: FAIL — `poll_duration_secs` not in request/response/record

**Step 3: Add `poll_duration_secs` to `RoomRecord` and `RoomRow`**

In `service/src/rooms/repo/rooms.rs`:

Add to `RoomRecord` (after `closed_at`):
```rust
pub poll_duration_secs: Option<i32>,
```

Add to `RoomRow` (after `closed_at`):
```rust
poll_duration_secs: Option<i32>,
```

Update `row_to_record` to include:
```rust
poll_duration_secs: row.poll_duration_secs,
```

Update all SELECT queries to include `poll_duration_secs` in the column list.

Update `create_room` to accept and bind `poll_duration_secs: Option<i32>`:
```sql
INSERT INTO rooms__rooms (name, description, eligibility_topic, poll_duration_secs)
VALUES ($1, $2, $3, $4)
RETURNING id, name, description, eligibility_topic, status, created_at, closed_at, poll_duration_secs
```

**Step 4: Add `closes_at` and `agenda_position` to `PollRecord` and `PollRow`**

In `service/src/rooms/repo/polls.rs`:

Add to `PollRecord` (after `closed_at`):
```rust
pub closes_at: Option<DateTime<Utc>>,
pub agenda_position: Option<i32>,
```

Add to `PollRow` (after `closed_at`):
```rust
closes_at: Option<DateTime<Utc>>,
agenda_position: Option<i32>,
```

Update `poll_row_to_record` to include both new fields.

Update all SELECT queries to include `closes_at, agenda_position` in column lists.

Update `create_poll` to accept `agenda_position: Option<i32>`:
```sql
INSERT INTO rooms__polls (room_id, question, description, agenda_position)
VALUES ($1, $2, $3, $4)
RETURNING id, room_id, question, description, status, created_at, activated_at, closed_at, closes_at, agenda_position
```

**Step 5: Update `RoomsRepo` trait**

In `service/src/rooms/repo/mod.rs`, update trait signatures:

```rust
async fn create_room(
    &self,
    name: &str,
    description: Option<&str>,
    eligibility_topic: &str,
    poll_duration_secs: Option<i32>,
) -> Result<RoomRecord, RoomRepoError>;

async fn create_poll(
    &self,
    room_id: Uuid,
    question: &str,
    description: Option<&str>,
    agenda_position: Option<i32>,
) -> Result<PollRecord, PollRepoError>;
```

Update `PgRoomsRepo` impl to pass through the new parameters.

**Step 6: Add new repo methods for agenda queries**

Add to `RoomsRepo` trait:

```rust
/// Get the next draft poll in a room's agenda (lowest agenda_position).
async fn next_agenda_poll(&self, room_id: Uuid) -> Result<Option<PollRecord>, PollRepoError>;

/// Get the next available agenda_position for a room.
async fn next_agenda_position(&self, room_id: Uuid) -> Result<i32, PollRepoError>;

/// Set closes_at on a poll.
async fn set_poll_closes_at(
    &self,
    poll_id: Uuid,
    closes_at: DateTime<Utc>,
) -> Result<(), PollRepoError>;

/// Get the active poll for a room (if any).
async fn get_active_poll(&self, room_id: Uuid) -> Result<Option<PollRecord>, PollRepoError>;
```

Implement in `polls.rs`:

```rust
pub async fn next_agenda_poll<'e, E>(
    executor: E,
    room_id: Uuid,
) -> Result<Option<PollRecord>, PollRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let row = sqlx::query_as::<_, PollRow>(
        r"
        SELECT id, room_id, question, description, status, created_at,
               activated_at, closed_at, closes_at, agenda_position
        FROM rooms__polls
        WHERE room_id = $1 AND status = 'draft' AND agenda_position IS NOT NULL
        ORDER BY agenda_position ASC
        LIMIT 1
        ",
    )
    .bind(room_id)
    .fetch_optional(executor)
    .await?;
    Ok(row.map(poll_row_to_record))
}

pub async fn next_agenda_position<'e, E>(
    executor: E,
    room_id: Uuid,
) -> Result<i32, PollRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let row: (Option<i32>,) = sqlx::query_as(
        r"
        SELECT MAX(agenda_position) FROM rooms__polls WHERE room_id = $1
        ",
    )
    .bind(room_id)
    .fetch_one(executor)
    .await?;
    Ok(row.0.map_or(0, |max| max + 1))
}

pub async fn set_poll_closes_at<'e, E>(
    executor: E,
    poll_id: Uuid,
    closes_at: DateTime<Utc>,
) -> Result<(), PollRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let result = sqlx::query("UPDATE rooms__polls SET closes_at = $1 WHERE id = $2")
        .bind(closes_at)
        .bind(poll_id)
        .execute(executor)
        .await?;
    if result.rows_affected() == 0 {
        return Err(PollRepoError::NotFound);
    }
    Ok(())
}

pub async fn get_active_poll<'e, E>(
    executor: E,
    room_id: Uuid,
) -> Result<Option<PollRecord>, PollRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let row = sqlx::query_as::<_, PollRow>(
        r"
        SELECT id, room_id, question, description, status, created_at,
               activated_at, closed_at, closes_at, agenda_position
        FROM rooms__polls
        WHERE room_id = $1 AND status = 'active'
        LIMIT 1
        ",
    )
    .bind(room_id)
    .fetch_optional(executor)
    .await?;
    Ok(row.map(poll_row_to_record))
}
```

**Step 7: Update service layer**

In `service/src/rooms/service.rs`:

Update `RoomsService` trait:
```rust
async fn create_room(
    &self,
    name: &str,
    description: Option<&str>,
    eligibility_topic: &str,
    poll_duration_secs: Option<i32>,
) -> Result<RoomRecord, RoomError>;
```

Update `DefaultRoomsService::create_room` to pass `poll_duration_secs` through to repo.

Update `create_poll` in service to auto-assign `agenda_position` by calling `repo.next_agenda_position(room_id)` before creating the poll.

**Step 8: Update HTTP layer**

In `service/src/rooms/http/mod.rs`:

Add to `CreateRoomRequest`:
```rust
pub poll_duration_secs: Option<i32>,
```

Update `create_room` handler to pass `req.poll_duration_secs` to service.

Add to `RoomResponse`:
```rust
pub poll_duration_secs: Option<i32>,
```

Update `room_to_response` to include the field.

Add to `PollResponse`:
```rust
pub closes_at: Option<String>,
```

Update `poll_to_response` to include `closes_at: p.closes_at.map(|t| t.to_rfc3339())`.

**Step 9: Run all tests**

Run: `just test-backend`
Expected: All tests pass (existing tests use the old call signatures — update them to add `None` for new params where needed)

**Step 10: Commit**

```bash
git add service/src/rooms/
git commit -m "feat(rooms): add poll_duration_secs, closes_at, agenda_position to room/poll models"
```

---

### Task 4: Lifecycle Service Methods

**Files:**
- Modify: `service/src/rooms/service.rs` (add lifecycle methods to trait + impl)

**Step 1: Write tests for lifecycle operations**

Create `service/tests/lifecycle_service_tests.rs`:

```rust
mod common;

use common::test_db::isolated_db;
use tc_test_macros::shared_runtime_test;
use std::sync::Arc;
use tinycongress_api::rooms::{
    repo::{PgRoomsRepo, RoomsRepo},
    service::{DefaultRoomsService, RoomsService},
};
use tinycongress_api::reputation::service::EndorsementService;

// Use a mock endorsement service (always returns true)
struct MockEndorsementService;
#[async_trait::async_trait]
impl EndorsementService for MockEndorsementService {
    async fn has_endorsement(&self, _user_id: uuid::Uuid, _topic: &str) -> Result<bool, anyhow::Error> {
        Ok(true)
    }
}

#[shared_runtime_test]
async fn test_lifecycle_close_and_advance() {
    let pool = isolated_db().await;
    let repo = Arc::new(PgRoomsRepo::new(pool.clone()));
    let endorsement = Arc::new(MockEndorsementService) as Arc<dyn EndorsementService>;
    let service = DefaultRoomsService::new(repo.clone() as Arc<dyn RoomsRepo>, endorsement);

    // Create a room with 1-hour cadence
    let room = service
        .create_room("Lifecycle Room", None, "identity_verified", Some(3600))
        .await
        .unwrap();

    // Create two polls (auto-assigned agenda positions)
    let poll1 = service
        .create_poll(room.id, "First question?", None)
        .await
        .unwrap();
    let poll2 = service
        .create_poll(room.id, "Second question?", None)
        .await
        .unwrap();

    // First poll should be auto-activated (room had no active poll)
    let poll1 = service.get_poll(poll1.id).await.unwrap();
    assert_eq!(poll1.status, "active");
    assert!(poll1.closes_at.is_some());

    // Second poll should be queued
    let poll2 = service.get_poll(poll2.id).await.unwrap();
    assert_eq!(poll2.status, "draft");
    assert_eq!(poll2.agenda_position, Some(1));

    // Close poll1 and advance
    service.close_poll_and_advance(room.id, poll1.id).await.unwrap();

    // poll1 should be closed
    let poll1 = service.get_poll(poll1.id).await.unwrap();
    assert_eq!(poll1.status, "closed");

    // poll2 should now be active with closes_at set
    let poll2 = service.get_poll(poll2.id).await.unwrap();
    assert_eq!(poll2.status, "active");
    assert!(poll2.closes_at.is_some());
}

#[shared_runtime_test]
async fn test_lifecycle_empty_agenda_no_error() {
    let pool = isolated_db().await;
    let repo = Arc::new(PgRoomsRepo::new(pool.clone()));
    let endorsement = Arc::new(MockEndorsementService) as Arc<dyn EndorsementService>;
    let service = DefaultRoomsService::new(repo.clone() as Arc<dyn RoomsRepo>, endorsement);

    let room = service
        .create_room("Empty Room", None, "identity_verified", Some(3600))
        .await
        .unwrap();

    let poll = service
        .create_poll(room.id, "Only question?", None)
        .await
        .unwrap();

    // Close the only poll — should succeed, no next poll to activate
    service.close_poll_and_advance(room.id, poll.id).await.unwrap();

    let poll = service.get_poll(poll.id).await.unwrap();
    assert_eq!(poll.status, "closed");
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p tinycongress-api --test lifecycle_service_tests`
Expected: FAIL — `close_poll_and_advance` doesn't exist

**Step 3: Add lifecycle methods to `RoomsService` trait**

In `service/src/rooms/service.rs`, add to trait:

```rust
/// Close the active poll and activate the next one from the agenda.
/// If the agenda is empty, the room sits idle.
async fn close_poll_and_advance(
    &self,
    room_id: Uuid,
    poll_id: Uuid,
) -> Result<(), PollError>;

/// List rooms that need agenda items (open rooms with no active poll
/// and no draft polls in the agenda).
async fn rooms_needing_content(&self) -> Result<Vec<RoomRecord>, RoomError>;
```

**Step 4: Implement `close_poll_and_advance`**

```rust
async fn close_poll_and_advance(
    &self,
    room_id: Uuid,
    poll_id: Uuid,
) -> Result<(), PollError> {
    // 1. Close the poll (idempotent — if already closed, skip)
    let poll = self.repo.get_poll(poll_id).await.map_err(|e| {
        if matches!(e, PollRepoError::NotFound) {
            PollError::PollNotFound
        } else {
            tracing::error!("Poll lookup failed: {e}");
            PollError::Internal("Internal server error".to_string())
        }
    })?;

    if poll.status == "active" {
        self.repo
            .update_poll_status(poll_id, "closed")
            .await
            .map_err(|e| {
                tracing::error!("Poll close failed: {e}");
                PollError::Internal("Internal server error".to_string())
            })?;
        tracing::info!(poll_id = %poll_id, room_id = %room_id, "closed poll");
    }

    // 2. Activate next from agenda
    let next = self.repo.next_agenda_poll(room_id).await.map_err(|e| {
        tracing::error!("Next agenda poll lookup failed: {e}");
        PollError::Internal("Internal server error".to_string())
    })?;

    if let Some(next_poll) = next {
        self.repo
            .update_poll_status(next_poll.id, "active")
            .await
            .map_err(|e| {
                tracing::error!("Poll activation failed: {e}");
                PollError::Internal("Internal server error".to_string())
            })?;

        // Set closes_at based on room cadence
        let room = self.repo.get_room(room_id).await.map_err(|e| {
            tracing::error!("Room lookup failed: {e}");
            PollError::Internal("Internal server error".to_string())
        })?;

        if let Some(duration_secs) = room.poll_duration_secs {
            let closes_at = chrono::Utc::now()
                + chrono::Duration::seconds(i64::from(duration_secs));
            self.repo
                .set_poll_closes_at(next_poll.id, closes_at)
                .await
                .map_err(|e| {
                    tracing::error!("Set closes_at failed: {e}");
                    PollError::Internal("Internal server error".to_string())
                })?;

            // Enqueue close event
            use super::repo::lifecycle_queue::{enqueue_lifecycle_event, LifecyclePayload};
            enqueue_lifecycle_event(
                self.pool(),
                &LifecyclePayload::ClosePoll {
                    poll_id: next_poll.id,
                    room_id,
                },
                i64::from(duration_secs),
            )
            .await
            .map_err(|e| {
                tracing::error!("Enqueue close event failed: {e}");
                PollError::Internal("Internal server error".to_string())
            })?;
        }

        tracing::info!(
            poll_id = %next_poll.id,
            room_id = %room_id,
            "activated next poll from agenda"
        );
    } else {
        tracing::info!(room_id = %room_id, "agenda empty, room idle");
    }

    Ok(())
}
```

**Important:** `DefaultRoomsService` needs access to the raw `PgPool` for queue operations. Add a `pool` field:

```rust
pub struct DefaultRoomsService {
    repo: Arc<dyn RoomsRepo>,
    endorsement_service: Arc<dyn EndorsementService>,
    pool: PgPool,
}
```

Update `new()` to accept `pool: PgPool` and update `main.rs` wiring accordingly.

**Step 5: Update `create_poll` to auto-activate first poll**

In `DefaultRoomsService::create_poll`, after creating the poll, check if room has cadence and no active poll:

```rust
async fn create_poll(
    &self,
    room_id: Uuid,
    question: &str,
    description: Option<&str>,
) -> Result<PollRecord, PollError> {
    if question.trim().is_empty() {
        return Err(PollError::Validation("Question cannot be empty".to_string()));
    }

    // Auto-assign agenda position
    let position = self.repo.next_agenda_position(room_id).await.map_err(|e| {
        tracing::error!("Agenda position lookup failed: {e}");
        PollError::Internal("Internal server error".to_string())
    })?;

    let poll = self.repo
        .create_poll(room_id, question.trim(), description, Some(position))
        .await
        .map_err(|e| {
            tracing::error!("Poll creation failed: {e}");
            PollError::Internal("Internal server error".to_string())
        })?;

    // Auto-activate if room has cadence and no active poll
    let room = self.repo.get_room(room_id).await.map_err(|e| {
        tracing::error!("Room lookup for auto-activate failed: {e}");
        PollError::Internal("Internal server error".to_string())
    })?;

    if room.poll_duration_secs.is_some() {
        let active = self.repo.get_active_poll(room_id).await.map_err(|e| {
            tracing::error!("Active poll check failed: {e}");
            PollError::Internal("Internal server error".to_string())
        })?;

        if active.is_none() {
            // This is the first poll — activate it
            self.repo.update_poll_status(poll.id, "active").await.map_err(|e| {
                tracing::error!("Auto-activate failed: {e}");
                PollError::Internal("Internal server error".to_string())
            })?;

            let duration_secs = room.poll_duration_secs.unwrap_or(0);
            let closes_at = chrono::Utc::now()
                + chrono::Duration::seconds(i64::from(duration_secs));
            self.repo.set_poll_closes_at(poll.id, closes_at).await.map_err(|e| {
                tracing::error!("Set closes_at on auto-activate failed: {e}");
                PollError::Internal("Internal server error".to_string())
            })?;

            // Enqueue close event
            use super::repo::lifecycle_queue::{enqueue_lifecycle_event, LifecyclePayload};
            enqueue_lifecycle_event(
                &self.pool,
                &LifecyclePayload::ClosePoll {
                    poll_id: poll.id,
                    room_id,
                },
                i64::from(duration_secs),
            )
            .await
            .map_err(|e| {
                tracing::error!("Enqueue close event failed: {e}");
                PollError::Internal("Internal server error".to_string())
            })?;

            // Re-fetch to return updated status
            return self.repo.get_poll(poll.id).await.map_err(|e| {
                tracing::error!("Poll re-fetch failed: {e}");
                PollError::Internal("Internal server error".to_string())
            });
        }
    }

    Ok(poll)
}
```

**Step 6: Run tests**

Run: `cargo test -p tinycongress-api --test lifecycle_service_tests`
Expected: PASS

**Step 7: Run all tests**

Run: `just test-backend`
Expected: PASS (fix any call-site breakage from changed signatures)

**Step 8: Commit**

```bash
git add service/src/rooms/ service/tests/
git commit -m "feat(rooms): add lifecycle service — close_poll_and_advance + auto-activate"
```

---

### Task 5: Background Queue Consumer

**Files:**
- Create: `service/src/rooms/lifecycle.rs` (consumer loop)
- Modify: `service/src/rooms/mod.rs` (add `pub mod lifecycle`)
- Modify: `service/src/main.rs` (spawn consumer)

**Step 1: Implement the consumer**

```rust
// service/src/rooms/lifecycle.rs
//! Background consumer for the lifecycle message queue.
//!
//! Polls `rooms__lifecycle_queue` for visible messages and dispatches
//! them to the rooms service for processing.

use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;

use super::repo::lifecycle_queue::{read_lifecycle_event, LifecyclePayload};
use super::service::RoomsService;

/// Spawn the lifecycle consumer as a background tokio task.
///
/// Polls the queue every `poll_interval` and processes messages
/// by delegating to the rooms service.
pub fn spawn_lifecycle_consumer(
    pool: PgPool,
    service: Arc<dyn RoomsService>,
    poll_interval: Duration,
) {
    tokio::spawn(async move {
        tracing::info!(
            poll_interval_ms = poll_interval.as_millis(),
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
                            if let Err(e) = service
                                .close_poll_and_advance(room_id, poll_id)
                                .await
                            {
                                tracing::warn!(
                                    poll_id = %poll_id,
                                    room_id = %room_id,
                                    error = %e,
                                    "close_poll_and_advance failed"
                                );
                            }
                        }
                        LifecyclePayload::ActivateNext { room_id } => {
                            // Create a dummy poll_id — close_poll_and_advance
                            // handles "already closed" idempotently.
                            // Better: extract activate_next as a separate service method.
                            if let Err(e) = service
                                .activate_next_from_agenda(room_id)
                                .await
                            {
                                tracing::warn!(
                                    room_id = %room_id,
                                    error = %e,
                                    "activate_next_from_agenda failed"
                                );
                            }
                        }
                    }
                }
                Ok(None) => {} // Queue empty, nothing to do
                Err(e) => {
                    tracing::warn!("lifecycle queue read failed: {e}");
                }
            }
        }
    });
}
```

**Step 2: Add `activate_next_from_agenda` to service trait**

In `service/src/rooms/service.rs`, add to trait:

```rust
/// Activate the next poll from a room's agenda (if any).
async fn activate_next_from_agenda(&self, room_id: Uuid) -> Result<(), PollError>;
```

Extract the "activate next" logic from `close_poll_and_advance` into this method, and have `close_poll_and_advance` call it after closing.

**Step 3: Register module**

In `service/src/rooms/mod.rs`, add:
```rust
pub mod lifecycle;
```

**Step 4: Wire into `main.rs`**

After `spawn_nonce_cleanup(pool_for_cleanup);` in `main.rs`, add:

```rust
// Lifecycle consumer — processes poll close/activate events
rooms::lifecycle::spawn_lifecycle_consumer(
    pool.clone(),
    rooms_service.clone(),
    Duration::from_secs(5),
);
```

Note: `rooms_service` must be cloned before it's moved into the app Extension. Reorder the wiring so the clone happens first.

**Step 5: Run all tests**

Run: `just test-backend`
Expected: PASS

**Step 6: Commit**

```bash
git add service/src/rooms/lifecycle.rs service/src/rooms/mod.rs service/src/main.rs
git commit -m "feat(rooms): add background lifecycle queue consumer"
```

---

### Task 6: HTTP Endpoints — Capacity and Agenda

**Files:**
- Modify: `service/src/rooms/http/mod.rs` (add new endpoints)
- Modify: `service/src/rooms/service.rs` (`rooms_needing_content` implementation)

**Step 1: Write test for capacity endpoint**

In `service/tests/rooms_handler_tests.rs`:

```rust
#[shared_runtime_test]
async fn test_capacity_endpoint_returns_rooms_needing_content() {
    let pool = isolated_db().await;
    let (app, keys, _) = signup_and_get_account("capacity_user", &pool).await;

    // Create room with cadence but no polls
    let body = serde_json::json!({
        "name": "Needy Room",
        "poll_duration_secs": 3600
    });
    let resp = app.clone().oneshot(build_authed_request(
        &keys, Method::POST, "/rooms", Some(body.to_string()),
    )).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Capacity should report this room
    let resp = app.clone().oneshot(
        Request::builder()
            .method(Method::GET)
            .uri("/rooms/capacity")
            .body(Body::empty())
            .unwrap(),
    ).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = parse_body(resp).await;
    let rooms = body.as_array().unwrap();
    assert!(rooms.iter().any(|r| r["name"] == "Needy Room"));
}
```

**Step 2: Run test to verify it fails**

Expected: 404 (route doesn't exist)

**Step 3: Implement `rooms_needing_content`**

In the repo, add a query that returns open rooms with `poll_duration_secs IS NOT NULL` and no active poll and no draft polls:

```rust
pub async fn rooms_needing_content<'e, E>(
    executor: E,
) -> Result<Vec<RoomRecord>, RoomRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let rows = sqlx::query_as::<_, RoomRow>(
        r"
        SELECT r.id, r.name, r.description, r.eligibility_topic,
               r.status, r.created_at, r.closed_at, r.poll_duration_secs
        FROM rooms__rooms r
        WHERE r.status = 'open'
          AND r.poll_duration_secs IS NOT NULL
          AND NOT EXISTS (
              SELECT 1 FROM rooms__polls p
              WHERE p.room_id = r.id AND p.status IN ('active', 'draft')
          )
        ORDER BY r.created_at ASC
        ",
    )
    .fetch_all(executor)
    .await?;
    Ok(rows.into_iter().map(row_to_record).collect())
}
```

Wire through repo trait → service → HTTP handler.

**Step 4: Add routes**

In the router:
```rust
.route("/rooms/capacity", get(get_capacity))
.route("/rooms/{room_id}/agenda", get(get_agenda))
```

**Important:** The `/rooms/capacity` route must be registered **before** `/rooms/{room_id}` to avoid the path parameter capturing "capacity" as a room_id.

**Step 5: Implement handlers**

```rust
async fn get_capacity(
    Extension(service): Extension<Arc<dyn RoomsService>>,
) -> impl IntoResponse {
    match service.rooms_needing_content().await {
        Ok(rooms) => {
            let rooms: Vec<_> = rooms.into_iter().map(room_to_response).collect();
            (StatusCode::OK, Json(rooms)).into_response()
        }
        Err(e) => room_error_response(e),
    }
}

async fn get_agenda(
    Extension(service): Extension<Arc<dyn RoomsService>>,
    Path(room_id): Path<Uuid>,
) -> impl IntoResponse {
    match service.get_agenda(room_id).await {
        Ok(polls) => {
            let polls: Vec<_> = polls.into_iter().map(poll_to_response).collect();
            (StatusCode::OK, Json(polls)).into_response()
        }
        Err(e) => poll_error_response(e),
    }
}
```

Add `get_agenda` to service trait and implement as listing draft polls ordered by agenda_position.

**Step 6: Run tests**

Run: `just test-backend`
Expected: PASS

**Step 7: Commit**

```bash
git add service/src/rooms/
git commit -m "feat(rooms): add capacity and agenda HTTP endpoints"
```

---

### Task 7: Sim Worker Integration

**Files:**
- Modify: `service/src/sim/content.rs` (use capacity endpoint, stop manual activation)
- Modify: `service/src/sim/client.rs` (add `get_capacity` method)
- Modify: `service/src/bin/sim.rs` (use capacity-based flow)

**Step 1: Add `get_capacity` to `SimClient`**

In `service/src/sim/client.rs`:

```rust
pub async fn get_capacity(&self) -> Result<Vec<RoomResponse>> {
    let resp = self
        .http
        .get(format!("{}/rooms/capacity", self.api_url))
        .send()
        .await?;
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("GET /rooms/capacity returned {}: {body}", resp.status()));
    }
    Ok(resp.json().await?)
}
```

**Step 2: Update `count_active_rooms` to use capacity**

Replace the current N+1 query pattern with the capacity endpoint:

```rust
pub async fn count_rooms_needing_content(client: &SimClient) -> Result<usize, anyhow::Error> {
    let rooms = client.get_capacity().await?;
    Ok(rooms.len())
}
```

**Step 3: Update `insert_sim_content` to stop manual activation**

Remove the `client.update_poll_status(... "active")` call from `insert_sim_content`. Polls auto-activate when they're the first in a room with cadence.

Also update `create_room` calls to pass `poll_duration_secs`.

**Step 4: Update sim main to use new flow**

In `service/src/bin/sim.rs`, replace the room-counting logic:

```rust
// Check capacity — rooms that need agenda items
let rooms_needing_content = client.get_capacity().await?;
tracing::info!(
    needing_content = rooms_needing_content.len(),
    target = config.target_rooms,
    "capacity check"
);

// Count total managed rooms
let all_rooms = client.list_rooms().await?;
let managed_rooms = all_rooms.len();

// Create new rooms if below target
if managed_rooms < config.target_rooms {
    let rooms_needed = config.target_rooms - managed_rooms;
    // ... generate and insert rooms with poll_duration_secs
}

// Fill empty agendas for existing rooms
if !rooms_needing_content.is_empty() {
    // ... generate polls for rooms that need them
}
```

**Step 5: Add `poll_duration_secs` to SimConfig**

```rust
#[serde(default = "default_poll_duration_secs")]
pub poll_duration_secs: i32,

fn default_poll_duration_secs() -> i32 {
    86400 // 24 hours
}
```

**Step 6: Run sim-related tests and lint**

Run: `just lint-backend && just test-backend`
Expected: PASS

**Step 7: Commit**

```bash
git add service/src/sim/ service/src/bin/sim.rs
git commit -m "feat(sim): use capacity endpoint, stop manual poll activation"
```

---

### Task 8: Integration Test — Full Lifecycle

**Files:**
- Create: `service/tests/lifecycle_integration_tests.rs`

**Step 1: Write end-to-end lifecycle test**

```rust
mod common;

use common::test_db::isolated_db;
use tc_test_macros::shared_runtime_test;
use std::sync::Arc;
use tinycongress_api::rooms::{
    repo::{PgRoomsRepo, RoomsRepo, lifecycle_queue::{enqueue_lifecycle_event, read_lifecycle_event, LifecyclePayload}},
    service::{DefaultRoomsService, RoomsService},
};
use tinycongress_api::reputation::service::EndorsementService;

struct MockEndorsementService;
#[async_trait::async_trait]
impl EndorsementService for MockEndorsementService {
    async fn has_endorsement(&self, _: uuid::Uuid, _: &str) -> Result<bool, anyhow::Error> {
        Ok(true)
    }
}

#[shared_runtime_test]
async fn test_full_lifecycle_rotation() {
    let pool = isolated_db().await;
    let repo = Arc::new(PgRoomsRepo::new(pool.clone()));
    let endorsement = Arc::new(MockEndorsementService) as Arc<dyn EndorsementService>;
    let service = DefaultRoomsService::new(
        repo.clone() as Arc<dyn RoomsRepo>,
        endorsement,
        pool.clone(),
    );

    // 1. Create room with 1-second cadence (fast for testing)
    let room = service
        .create_room("Rotation Room", None, "identity_verified", Some(1))
        .await
        .unwrap();

    // 2. Push 3 polls onto agenda
    let p1 = service.create_poll(room.id, "Q1?", None).await.unwrap();
    let p2 = service.create_poll(room.id, "Q2?", None).await.unwrap();
    let p3 = service.create_poll(room.id, "Q3?", None).await.unwrap();

    // p1 should be auto-activated
    let p1 = service.get_poll(p1.id).await.unwrap();
    assert_eq!(p1.status, "active");

    // 3. A close event should be in the queue (delayed 1 second)
    // Wait for it to become visible
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let msg = read_lifecycle_event(&pool).await.unwrap();
    assert!(msg.is_some(), "close event should be visible after delay");

    // 4. Process it via service
    service.close_poll_and_advance(room.id, p1.id).await.unwrap();

    // p1 closed, p2 active
    let p1 = service.get_poll(p1.id).await.unwrap();
    assert_eq!(p1.status, "closed");
    let p2 = service.get_poll(p2.id).await.unwrap();
    assert_eq!(p2.status, "active");

    // 5. Advance again
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let msg = read_lifecycle_event(&pool).await.unwrap();
    assert!(msg.is_some());
    service.close_poll_and_advance(room.id, p2.id).await.unwrap();

    let p3 = service.get_poll(p3.id).await.unwrap();
    assert_eq!(p3.status, "active");

    // 6. Final advance — agenda empty
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let msg = read_lifecycle_event(&pool).await.unwrap();
    assert!(msg.is_some());
    service.close_poll_and_advance(room.id, p3.id).await.unwrap();

    let p3 = service.get_poll(p3.id).await.unwrap();
    assert_eq!(p3.status, "closed");

    // No more messages — room is idle
    let msg = read_lifecycle_event(&pool).await.unwrap();
    assert!(msg.is_none());

    // 7. Room should appear in capacity
    let needy = service.rooms_needing_content().await.unwrap();
    assert!(needy.iter().any(|r| r.id == room.id));
}
```

**Step 2: Run the integration test**

Run: `cargo test -p tinycongress-api --test lifecycle_integration_tests -- --nocapture`
Expected: PASS

**Step 3: Commit**

```bash
git add service/tests/lifecycle_integration_tests.rs
git commit -m "test(rooms): add full lifecycle rotation integration test"
```

---

### Task 9: Final Checks and Cleanup

**Files:**
- Various (fix any remaining compilation issues, update existing tests)

**Step 1: Run full test suite**

Run: `just test`
Expected: PASS

**Step 2: Run linting**

Run: `just lint`
Expected: PASS (fix any clippy/formatting issues)

**Step 3: Commit any fixes**

```bash
git add -u
git commit -m "fix: address lint and test issues from lifecycle feature"
```

**Step 4: Review all changes**

Run: `git diff master --stat`
Verify: Only expected files changed, no unrelated modifications.

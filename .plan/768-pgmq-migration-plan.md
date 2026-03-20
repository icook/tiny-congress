# Migrate Homegrown Queues to pgmq — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace `rooms__lifecycle_queue` and `trust__action_queue` with pgmq queues to get automatic crash recovery via visibility timeouts.

**Architecture:** Two pgmq queues (`rooms__lifecycle`, `trust__actions`) replace the homegrown implementations. The lifecycle queue is a pure 1:1 swap. The trust queue splits into a pgmq queue (transient processing) + `trust__action_log` table (permanent business state for quotas and audit). The existing `pgmq.rs` module is generalized to support multiple queue names.

**Tech Stack:** Rust, sqlx, pgmq (Postgres extension), tokio

---

## Reference Files

These files contain patterns and types you'll need to understand:

- `crates/tc-engine-polling/src/repo/pgmq.rs` — existing pgmq wrapper (bot queue), the target pattern
- `crates/tc-engine-polling/src/repo/lifecycle_queue.rs` — lifecycle queue being replaced
- `crates/tc-engine-polling/src/lifecycle.rs` — lifecycle consumer being updated
- `crates/tc-engine-polling/src/service.rs:262,401` — lifecycle enqueue call sites
- `crates/tc-engine-polling/src/engine.rs:80` — lifecycle consumer spawn site
- `crates/tc-engine-polling/src/bot/worker.rs` — bot worker (reference pgmq consumer pattern)
- `service/src/trust/repo/action_queue.rs` — trust queue functions being replaced
- `service/src/trust/repo/mod.rs` — TrustRepo trait (needs trait method changes)
- `service/src/trust/worker.rs` — trust worker being rewritten
- `service/src/trust/service.rs` — trust service (enqueue call sites)
- `service/src/main.rs:336-343` — trust worker spawn site
- `service/migrations/24_pgmq_bot_queue.sql` — existing pgmq migration (pattern)

---

### Task 1: Generalize pgmq helper functions

The existing `pgmq.rs` hardcodes `QUEUE_NAME = "rooms__bot_tasks"`. Generalize the send/read/archive/delete functions to accept a queue name parameter so all three queues share the same helpers.

**Files:**
- Modify: `crates/tc-engine-polling/src/repo/pgmq.rs`
- Modify: `crates/tc-engine-polling/src/bot/worker.rs` (update call sites)

**Step 1: Refactor pgmq functions to take queue_name parameter**

Change each function signature to accept `queue_name: &str` as the first parameter instead of using the `QUEUE_NAME` constant. Keep the constant for bot-queue callers.

In `pgmq.rs`, change:
```rust
pub async fn send_task(pool: &PgPool, task: &BotTask) -> Result<i64, sqlx::Error> {
```
to:
```rust
pub async fn send(pool: &PgPool, queue_name: &str, payload: &serde_json::Value) -> Result<i64, sqlx::Error> {
```

Apply the same pattern to `send_task_delayed` → `send_delayed`, `read_task` → `read`, `delete_task` → `delete`, `archive_task` → `archive`. The payload is now raw `serde_json::Value` — callers handle serialization of their own types.

Keep the `BotTask`-specific convenience wrappers as thin functions that call the generic ones:
```rust
pub async fn send_task(pool: &PgPool, task: &BotTask) -> Result<i64, sqlx::Error> {
    let payload = serde_json::to_value(task)
        .map_err(|e| sqlx::Error::Protocol(format!("failed to serialize BotTask: {e}")))?;
    send(pool, QUEUE_NAME, &payload).await
}
```

**Step 2: Update bot worker call sites**

In `bot/worker.rs`, update:
- `pgmq::read_task(&pool, VT)` → `pgmq::read(&pool, pgmq::QUEUE_NAME, VT)` (or keep using `read_task` convenience wrapper)
- `pgmq::archive_task(&pool, msg_id)` → `pgmq::archive(&pool, pgmq::QUEUE_NAME, msg_id)` (or keep wrapper)

Since the bot worker already uses the convenience wrappers, no changes needed in `worker.rs` if we keep the wrappers.

**Step 3: Verify compilation**

Run: `cargo check -p tc-engine-polling`
Expected: compiles with no errors

**Step 4: Run existing tests**

Run: `cargo test -p tc-engine-polling`
Expected: all tests pass (the unit tests for `BotTask` serialization should still work)

**Step 5: Commit**

```bash
git add crates/tc-engine-polling/src/repo/pgmq.rs
git commit -m "refactor: generalize pgmq helpers to accept queue name parameter"
```

---

### Task 2: SQL Migration

Create migration 25 that sets up the new pgmq queues, renames the trust table, and drops the lifecycle table. Handle in-flight messages.

**Files:**
- Create: `service/migrations/25_pgmq_queue_migration.sql`

**Step 1: Write the migration**

```sql
-- Migration 25: Migrate homegrown queues to pgmq
--
-- Creates pgmq queues for lifecycle and trust actions, renames
-- trust__action_queue to trust__action_log, and drops the old
-- lifecycle queue table.

-- 1. Create the two new pgmq queues
SELECT pgmq.create('rooms__lifecycle');
SELECT pgmq.create('trust__actions');

-- 2. Migrate any in-flight lifecycle messages to the pgmq queue.
--    visible_at is converted to a delay in seconds from now.
--    Messages already past their visible_at get delay=0.
INSERT INTO pgmq.q_rooms__lifecycle (vt, message)
SELECT
    GREATEST(visible_at, now()),
    payload
FROM rooms__lifecycle_queue
ORDER BY id;

-- 3. Drop the old lifecycle queue table (data migrated above)
DROP TABLE IF EXISTS rooms__lifecycle_queue;

-- 4. Rename trust__action_queue → trust__action_log
--    In-flight 'pending' rows stay in the table and will be picked up
--    by a one-time drain in the worker startup code.
ALTER TABLE trust__action_queue RENAME TO trust__action_log;

-- 5. Rename indexes to match new table name
ALTER INDEX IF EXISTS idx_action_queue_pending
    RENAME TO idx_action_log_pending;
ALTER INDEX IF EXISTS idx_action_queue_actor_date
    RENAME TO idx_action_log_actor_date;
```

**Step 2: Verify migration applies**

Run: `cargo test -p tc-server --test db_tests` (or whichever test bootstraps the DB)
Expected: migration applies without errors

**Step 3: Commit**

```bash
git add service/migrations/25_pgmq_queue_migration.sql
git commit -m "feat: add migration to create pgmq queues and retire homegrown queue tables (#768)"
```

---

### Task 3: Migrate lifecycle queue to pgmq

Replace the lifecycle queue repo and consumer with pgmq-backed equivalents.

**Files:**
- Modify: `crates/tc-engine-polling/src/repo/lifecycle_queue.rs`
- Modify: `crates/tc-engine-polling/src/lifecycle.rs`
- Modify: `crates/tc-engine-polling/src/service.rs` (enqueue call sites)

**Step 1: Rewrite lifecycle_queue.rs**

Keep `LifecyclePayload` and `LifecycleMessage` types. Replace the queue operations:

```rust
//! Lifecycle queue persistence — pgmq-backed
//!
//! Wraps the `rooms__lifecycle` pgmq queue. Messages use visibility-timeout
//! based redelivery for crash recovery.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use super::pgmq::{self, PgmqMessage};

/// pgmq queue name for lifecycle events.
pub const QUEUE_NAME: &str = "rooms__lifecycle";

/// Maximum delivery attempts before a message is treated as poison.
const MAX_RETRIES: i32 = 3;

/// Visibility timeout in seconds (how long a message is hidden while processing).
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
/// Returns `None` when the queue is empty. The message is hidden for
/// `VISIBILITY_TIMEOUT_SECS`; if not archived within that window it
/// becomes visible again for automatic retry.
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
pub async fn archive_lifecycle_event(pool: &PgPool, msg_id: i64) -> Result<(), sqlx::Error> {
    pgmq::archive(pool, QUEUE_NAME, msg_id).await
}

/// Check if a message has exceeded the retry limit.
pub fn is_poison(msg: &LifecycleMessage) -> bool {
    msg.read_ct > MAX_RETRIES
}
```

Remove: `delete_lifecycle_event` (unused), `QueueRow`, `row_to_message`.

The function signature for `enqueue_lifecycle_event` changes: the first parameter was generic `E: sqlx::Executor` and is now `&PgPool`. Both call sites already pass `&self.pool`, so this is compatible.

**Step 2: Update lifecycle consumer**

In `lifecycle.rs`, update the consumer loop to use the pgmq pattern (read → process → archive):

```rust
//! Background consumer for the lifecycle message queue.

use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;

use crate::repo::lifecycle_queue::{
    archive_lifecycle_event, is_poison, read_lifecycle_event, LifecyclePayload,
};
use crate::service::PollingService;

/// Spawn the lifecycle consumer as a background tokio task.
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
                    // Poison message — archive and skip
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
                        LifecyclePayload::ClosePoll { poll_id, room_id } => {
                            polling_service
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
                                .is_ok()
                        }
                        LifecyclePayload::ActivateNext { room_id } => {
                            polling_service
                                .activate_next_from_agenda(room_id)
                                .await
                                .map_err(|e| {
                                    tracing::warn!(
                                        room_id = %room_id,
                                        error = %e,
                                        "activate_next_from_agenda failed"
                                    );
                                })
                                .is_ok()
                        }
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
```

**Step 3: Update enqueue call sites in service.rs**

At lines 262 and 401, the calls already pass `&self.pool`. The only change: the function no longer accepts a generic executor, so if the compiler complains about type inference, just ensure the calls pass `&self.pool` directly.

**Step 4: Verify compilation**

Run: `cargo check -p tc-engine-polling`
Expected: compiles cleanly

**Step 5: Run tests**

Run: `cargo test -p tc-engine-polling`
Expected: all tests pass

**Step 6: Commit**

```bash
git add crates/tc-engine-polling/src/repo/lifecycle_queue.rs crates/tc-engine-polling/src/lifecycle.rs crates/tc-engine-polling/src/service.rs
git commit -m "feat: migrate lifecycle queue to pgmq with crash recovery (#768)"
```

---

### Task 4: Migrate trust queue to pgmq

Split the trust action queue into pgmq queue + action log table. Update repo, trait, worker, and service.

**Files:**
- Modify: `service/src/trust/repo/action_queue.rs`
- Modify: `service/src/trust/repo/mod.rs` (TrustRepo trait + PgTrustRepo impl)
- Modify: `service/src/trust/worker.rs`
- Modify: `service/src/main.rs` (worker construction)

**Step 1: Rewrite action_queue.rs**

The file now manages the `trust__action_log` table (business state) and the `trust__actions` pgmq queue (processing). `claim_pending_actions` is removed entirely.

```rust
use sqlx::PgPool;
use uuid::Uuid;

use super::{ActionRecord, TrustRepoError};
use tc_engine_polling::repo::pgmq;

/// pgmq queue name for trust actions.
pub const QUEUE_NAME: &str = "trust__actions";

/// Insert a log row and enqueue a pgmq message referencing it.
pub(super) async fn enqueue_action(
    pool: &PgPool,
    actor_id: Uuid,
    action_type: &str,
    payload: &serde_json::Value,
) -> Result<ActionRecord, TrustRepoError> {
    // 1. Insert into the action log
    let record = sqlx::query_as::<_, ActionRecord>(
        "INSERT INTO trust__action_log (actor_id, action_type, payload) \
         VALUES ($1, $2, $3) \
         RETURNING *",
    )
    .bind(actor_id)
    .bind(action_type)
    .bind(payload)
    .fetch_one(pool)
    .await?;

    // 2. Send pgmq message with reference to log row
    let msg_payload = serde_json::json!({ "log_id": record.id });
    pgmq::send(pool, QUEUE_NAME, &msg_payload)
        .await
        .map_err(|e| TrustRepoError::Database(e))?;

    Ok(record)
}

pub(super) async fn count_daily_actions(
    pool: &PgPool,
    actor_id: Uuid,
) -> Result<i64, TrustRepoError> {
    let row = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM trust__action_log \
         WHERE actor_id = $1 AND quota_date = CURRENT_DATE",
    )
    .bind(actor_id)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

/// Look up an action log record by ID.
pub(super) async fn get_action(
    pool: &PgPool,
    action_id: Uuid,
) -> Result<ActionRecord, TrustRepoError> {
    let record = sqlx::query_as::<_, ActionRecord>(
        "SELECT * FROM trust__action_log WHERE id = $1",
    )
    .bind(action_id)
    .fetch_optional(pool)
    .await?
    .ok_or(TrustRepoError::NotFound)?;

    Ok(record)
}

pub(super) async fn complete_action(pool: &PgPool, action_id: Uuid) -> Result<(), TrustRepoError> {
    sqlx::query(
        "UPDATE trust__action_log \
         SET status = 'completed', processed_at = now() \
         WHERE id = $1",
    )
    .bind(action_id)
    .execute(pool)
    .await?;

    Ok(())
}

pub(super) async fn fail_action(
    pool: &PgPool,
    action_id: Uuid,
    error: &str,
) -> Result<(), TrustRepoError> {
    sqlx::query(
        "UPDATE trust__action_log \
         SET status = 'failed', error_message = $2, processed_at = now() \
         WHERE id = $1",
    )
    .bind(action_id)
    .bind(error)
    .execute(pool)
    .await?;

    Ok(())
}
```

**Step 2: Update TrustRepo trait**

In `service/src/trust/repo/mod.rs`:

Remove from the trait:
```rust
async fn claim_pending_actions(&self, limit: i64) -> Result<Vec<ActionRecord>, TrustRepoError>;
```

Add to the trait:
```rust
async fn get_action(&self, action_id: Uuid) -> Result<ActionRecord, TrustRepoError>;
```

Update `PgTrustRepo` impl:
- Remove `claim_pending_actions` delegation
- Add `get_action` delegation to `action_queue::get_action`

**Step 3: Rewrite trust worker**

The worker switches from batch-claim to single-message pgmq consumption, matching the bot worker pattern.

```rust
//! Background worker — processes trust actions via pgmq queue.

use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use uuid::Uuid;

use crate::reputation::repo::ReputationRepo;
use crate::trust::engine::TrustEngine;
use crate::trust::repo::{ActionRecord, TrustRepo};

use tc_engine_polling::repo::pgmq;

/// pgmq queue name (must match action_queue.rs).
const QUEUE_NAME: &str = "trust__actions";

/// Maximum delivery attempts before treating as poison.
const MAX_RETRIES: i32 = 3;

/// Visibility timeout in seconds.
const VISIBILITY_TIMEOUT_SECS: i32 = 120;

/// Poll interval when queue is empty.
const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Background worker that processes trust actions from the pgmq queue.
pub struct TrustWorker {
    pool: PgPool,
    trust_repo: Arc<dyn TrustRepo>,
    reputation_repo: Arc<dyn ReputationRepo>,
    trust_engine: Arc<TrustEngine>,
}

impl TrustWorker {
    #[must_use]
    pub fn new(
        pool: PgPool,
        trust_repo: Arc<dyn TrustRepo>,
        reputation_repo: Arc<dyn ReputationRepo>,
        trust_engine: Arc<TrustEngine>,
    ) -> Self {
        Self {
            pool,
            trust_repo,
            reputation_repo,
            trust_engine,
        }
    }

    /// Run the worker loop indefinitely.
    pub async fn run(self: Arc<Self>) {
        tracing::info!("trust worker started (pgmq mode)");
        loop {
            match pgmq::read(&self.pool, QUEUE_NAME, VISIBILITY_TIMEOUT_SECS).await {
                Ok(Some(msg)) => {
                    if msg.read_ct > MAX_RETRIES {
                        tracing::warn!(
                            msg_id = msg.msg_id,
                            read_ct = msg.read_ct,
                            "archiving poison trust action after {} retries",
                            MAX_RETRIES
                        );
                        // Also mark the log entry as failed
                        if let Some(log_id) = self.extract_log_id(&msg.message) {
                            let _ = self.trust_repo.fail_action(
                                log_id,
                                &format!("poison message after {} retries", msg.read_ct),
                            ).await;
                        }
                        if let Err(e) = pgmq::archive(&self.pool, QUEUE_NAME, msg.msg_id).await {
                            tracing::warn!(msg_id = msg.msg_id, error = %e, "failed to archive poison message");
                        }
                        continue;
                    }

                    // Extract log_id from pgmq message
                    let log_id = match self.extract_log_id(&msg.message) {
                        Some(id) => id,
                        None => {
                            tracing::warn!(
                                msg_id = msg.msg_id,
                                "malformed trust action message; archiving"
                            );
                            if let Err(e) = pgmq::archive(&self.pool, QUEUE_NAME, msg.msg_id).await {
                                tracing::warn!(msg_id = msg.msg_id, error = %e, "failed to archive malformed message");
                            }
                            continue;
                        }
                    };

                    // Look up the action log record
                    let action = match self.trust_repo.get_action(log_id).await {
                        Ok(a) => a,
                        Err(e) => {
                            tracing::error!(log_id = %log_id, error = %e, "failed to fetch action log record");
                            // Let VT expire for retry
                            continue;
                        }
                    };

                    // Process the action
                    match self.process_action(&action).await {
                        Ok(()) => {
                            if let Err(e) = self.trust_repo.complete_action(action.id).await {
                                tracing::error!(action_id = %action.id, "failed to mark action complete: {e}");
                            }
                            if let Err(e) = pgmq::archive(&self.pool, QUEUE_NAME, msg.msg_id).await {
                                tracing::warn!(msg_id = msg.msg_id, error = %e, "failed to archive completed action");
                            }
                        }
                        Err(e) => {
                            tracing::error!(
                                action_id = %action.id,
                                action_type = %action.action_type,
                                "action processing error: {e}"
                            );
                            if let Err(fe) = self.trust_repo.fail_action(action.id, &e.to_string()).await {
                                tracing::error!(action_id = %action.id, "failed to mark action failed: {fe}");
                            }
                            // Archive on failure (action is logged as failed)
                            if let Err(e) = pgmq::archive(&self.pool, QUEUE_NAME, msg.msg_id).await {
                                tracing::warn!(msg_id = msg.msg_id, error = %e, "failed to archive failed action");
                            }
                        }
                    }
                }
                Ok(None) => {
                    tokio::time::sleep(POLL_INTERVAL).await;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "trust action queue read failed");
                    tokio::time::sleep(POLL_INTERVAL).await;
                }
            }
        }
    }

    fn extract_log_id(&self, message: &serde_json::Value) -> Option<Uuid> {
        message["log_id"]
            .as_str()
            .and_then(|s| s.parse::<Uuid>().ok())
    }

    // process_action is unchanged from the current implementation
    async fn process_action(&self, action: &ActionRecord) -> Result<(), anyhow::Error> {
        // ... (copy existing process_action body unchanged) ...
    }
}

// parse_uuid helper — unchanged
fn parse_uuid(payload: &serde_json::Value, key: &str) -> Result<Uuid, anyhow::Error> {
    // ... (copy existing) ...
}
```

**Step 4: Update TrustWorker construction in main.rs**

The constructor changes: it now takes `pool: PgPool` and drops `batch_size`/`batch_interval_secs`.

At `service/src/main.rs:336-343`, update:
```rust
// Before:
let trust_worker = Arc::new(TrustWorker::new(
    trust_repo_for_worker,
    reputation_repo_for_worker,
    trust_engine,
    config.trust.batch_size,
    config.trust.batch_interval_secs,
));

// After:
let trust_worker = Arc::new(TrustWorker::new(
    pool.clone(),
    trust_repo_for_worker,
    reputation_repo_for_worker,
    trust_engine,
));
```

Also remove `batch_size` and `batch_interval_secs` from the trust config struct if they're no longer used (check `service/src/config.rs` or wherever trust config is defined).

**Step 5: Handle in-flight pending actions**

The migration renames the table but any `status = 'pending'` rows won't have pgmq messages. Add a one-time drain at worker startup that queries `trust__action_log WHERE status = 'pending'` and enqueues pgmq messages for each:

In the `run` method, before the main loop:
```rust
// One-time drain: enqueue pgmq messages for any pre-migration pending actions
match self.drain_legacy_pending().await {
    Ok(count) if count > 0 => {
        tracing::info!(count, "drained legacy pending actions to pgmq");
    }
    Err(e) => {
        tracing::error!(error = %e, "failed to drain legacy pending actions");
    }
    _ => {}
}
```

```rust
async fn drain_legacy_pending(&self) -> Result<usize, anyhow::Error> {
    let pending: Vec<ActionRecord> = sqlx::query_as(
        "SELECT * FROM trust__action_log WHERE status = 'pending' ORDER BY created_at"
    )
    .fetch_all(&self.pool)
    .await?;

    let count = pending.len();
    for action in &pending {
        let msg_payload = serde_json::json!({ "log_id": action.id });
        pgmq::send(&self.pool, QUEUE_NAME, &msg_payload).await?;
    }
    Ok(count)
}
```

This is safe because pgmq deduplicates by message content? No — pgmq does not deduplicate. But since these are pre-migration rows that never had pgmq messages, there's no risk of duplicates. The drain runs once per worker startup, and pending rows get processed and marked completed/failed, so subsequent startups find nothing to drain. This code can be removed after one deploy cycle.

**Step 6: Verify compilation**

Run: `cargo check`
Expected: compiles cleanly (full workspace check since we touch both crates)

**Step 7: Run tests**

Run: `cargo test`
Expected: all tests pass. Some trust worker tests may need updating if they mock `claim_pending_actions`.

**Step 8: Commit**

```bash
git add service/src/trust/repo/action_queue.rs service/src/trust/repo/mod.rs service/src/trust/worker.rs service/src/main.rs
git commit -m "feat: migrate trust action queue to pgmq with action log split (#768)"
```

---

### Task 5: Cleanup and verification

Remove dead code, verify lint passes, run full test suite.

**Files:**
- Modify: `crates/tc-engine-polling/src/repo/mod.rs` (if re-exports need updating)
- Modify: `service/src/rooms/repo/mod.rs` (if re-exports need updating)
- Possibly modify: trust config struct (remove batch_size/batch_interval_secs)

**Step 1: Remove unused re-exports**

Check `crates/tc-engine-polling/src/repo/mod.rs` and `service/src/rooms/repo/mod.rs` for any re-exports of removed functions (`delete_lifecycle_event`). Remove them.

**Step 2: Clean up trust config**

If `config.trust.batch_size` and `config.trust.batch_interval_secs` are no longer used anywhere, remove them from the config struct and any env var loading.

**Step 3: Run lint**

Run: `just lint`
Expected: no warnings or errors

**Step 4: Run full test suite**

Run: `just test`
Expected: all tests pass

**Step 5: Run static analysis**

Run: `just lint-static`
Expected: passes

**Step 6: Commit cleanup**

```bash
git add -u
git commit -m "chore: remove dead queue code and unused trust worker config (#768)"
```

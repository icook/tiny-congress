# Migrate Homegrown Queues to pgmq

> **Issue:** #768

**Goal:** Replace both homegrown queues (`rooms__lifecycle_queue`, `trust__action_queue`) with pgmq to get automatic crash recovery via visibility timeouts.

**Architecture:** Two pgmq queues replace the homegrown implementations. The lifecycle queue is a pure 1:1 swap. The trust queue splits into a pgmq queue (transient processing) + action log table (permanent business state for quotas and audit).

---

## Lifecycle Queue → pgmq `rooms__lifecycle`

Pure queue swap. No business state concerns.

- Create pgmq queue `rooms__lifecycle` in a migration
- Drop `rooms__lifecycle_queue` table
- `enqueue_lifecycle_event()` → `pgmq.send()` (with delay for `ClosePoll`)
- `read_lifecycle_event()` → `pgmq.read()` with visibility timeout (e.g. 60s)
- On success → `pgmq.archive()`. On failure → let VT expire for automatic retry
- Consumer loop unchanged (poll every 5s), just swaps the underlying calls
- Add `read_ct > MAX_RETRIES` poison message handling (archive + log), same pattern as bot worker

## Trust Queue → pgmq `trust__actions` + `trust__action_log`

Split queue from ledger.

### `trust__action_log` (rename of existing table)

Keep all columns: `id`, `actor_id`, `action_type`, `payload`, `status` (pending/completed/failed), `error_message`, `quota_date`, `processed_at`, `created_at`. Keeps the `(actor_id, quota_date)` index for quota enforcement.

### pgmq queue `trust__actions`

Message payload: `{ log_id: Uuid }` — reference to the action log row.

### Flow

1. Service layer checks quota against `trust__action_log`
2. Inserts log row with `status='pending'`
3. Sends pgmq message with `log_id`
4. Worker reads pgmq message, processes action, updates log row to completed/failed
5. Archives pgmq message. On crash → VT expires → automatic redelivery

## Migration Strategy

Single SQL migration that:
1. Creates both pgmq queues (`rooms__lifecycle`, `trust__actions`)
2. Renames `trust__action_queue` → `trust__action_log`
3. Drops `rooms__lifecycle_queue`
4. Handles in-flight messages: process existing pending rows before switching consumers

## Testing

- Unit tests for the new repo functions (send/read/archive wrappers)
- Integration test: enqueue → process → verify log state
- Verify quota enforcement still works against the log table

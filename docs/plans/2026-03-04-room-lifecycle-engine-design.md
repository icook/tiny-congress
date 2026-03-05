# Room Lifecycle Engine Design

**Issue:** #466 — Maintain target active room count with automatic lifecycle
**Date:** 2026-03-04
**Status:** Approved

## Overview

Rooms are long-lived containers (like subreddits) with an **agenda** — a FIFO queue of polls. A lifecycle engine rotates polls on a configurable cadence: when the active poll's time expires, it closes and the next draft poll activates. The sim becomes a pure content creator that fills agenda slots; it never manages lifecycle.

## Architecture Decisions

- **Lifecycle is a core rooms concern**, not a sim concern. Any content source (sim, admin, community) can push polls onto an agenda; the rooms engine handles all state transitions.
- **pgmq** (Postgres-native message queue) drives lifecycle events. Chosen over background sweep for: exact-time delivery, natural horizontal scaling, built-in retry/observability, and event-driven decomposition.
- **Room-level cadence** (`poll_duration_secs`) determines rotation interval. All polls in a room run for the same duration. Per-poll overrides deferred (YAGNI).
- **No auto-archiving.** Rooms stay `open` indefinitely. When a room's last poll closes and the agenda is empty, it sits idle awaiting new content. Archiving is a separate future concern.

## Data Model Changes

### Migration

```sql
-- Room-level rotation config
ALTER TABLE rooms__rooms
  ADD COLUMN poll_duration_secs INTEGER;  -- NULL = no auto-rotation

-- Poll scheduling
ALTER TABLE rooms__polls
  ADD COLUMN closes_at TIMESTAMPTZ,       -- set on activation: activated_at + room.poll_duration_secs
  ADD COLUMN agenda_position INTEGER;     -- FIFO ordering within room
```

- `poll_duration_secs` on the room is the cadence. When a poll activates, `closes_at = now() + poll_duration_secs`.
- `agenda_position` orders draft polls within a room. Lowest position activates next. Auto-incremented on poll creation (max+1 for room).
- Existing data: both columns nullable, backward compatible. Rooms without `poll_duration_secs` behave as today (manual transitions only).

## pgmq Integration

### Queue

One queue: `room_lifecycle`

### Message Types

| Type | Payload | Trigger |
|------|---------|---------|
| `ClosePoll` | `{ poll_id, room_id }` | Enqueued with delay when a poll activates. Fires at `closes_at`. |
| `ActivateNext` | `{ room_id }` | Enqueued immediately after closing a poll. |

### Consumer Flow

```
ClosePoll fires →
  1. Verify poll is still active (idempotent guard)
  2. Close the poll (status='closed', closed_at=now())
  3. Preserve votes and results as immutable artifacts
  4. Enqueue ActivateNext { room_id }

ActivateNext fires →
  1. Query next draft poll by agenda_position for room_id
  2. If found:
     a. Activate poll (status='active', activated_at=now())
     b. Set closes_at = now() + room.poll_duration_secs
     c. Enqueue ClosePoll { poll_id, room_id } with delay = poll_duration_secs
  3. If empty: no-op (room sits idle, reported via capacity endpoint)
```

### Consumer Runtime

- Runs as a background `tokio::spawn` task inside the API server
- Polls the pgmq queue every 1–5 seconds
- The rooms service exposes lifecycle methods; the consumer is the trigger mechanism
- pgmq's atomic `read()` ensures safe multi-instance operation

## API Changes

| Endpoint | Change | Purpose |
|----------|--------|---------|
| `POST /rooms` | Add optional `poll_duration_secs` in body | Set room cadence at creation |
| `POST /rooms/{id}/polls` | Auto-assign `agenda_position` (max+1) | Push poll onto room's agenda |
| `POST /rooms/{id}/polls` | Auto-activate if room has no active poll | First poll activates immediately |
| `GET /rooms/{id}/agenda` | **New** | List draft polls in FIFO order |
| `GET /rooms/capacity` | **New** | Return rooms needing agenda items |

### Poll creation behavior change

When a poll is created via `POST /rooms/{id}/polls`:
1. Poll created as `draft` with auto-assigned `agenda_position`
2. If the room has `poll_duration_secs` set AND no currently active poll:
   - Auto-activate the poll
   - Set `closes_at = now() + poll_duration_secs`
   - Enqueue `ClosePoll` to pgmq
3. Otherwise: poll queues in the agenda

## Sim Worker Changes

The sim becomes a pure content creator:

1. Check `GET /rooms/capacity` to find rooms needing agenda items
2. If fewer than `target_rooms` rooms exist, create new rooms (with `poll_duration_secs`)
3. For rooms needing content, generate polls via LLM and POST them
4. Sim **never** calls status change endpoints — the rooms engine handles activation and closing

The sim's `target_rooms` config becomes "target number of rooms in the system" rather than "target active rooms." Capacity is about agenda depth, not room count.

## Error Handling

- **Concurrent consumers:** pgmq `read()` is atomic. Safe with multiple API instances.
- **Vote race on close:** Vote handler already checks `poll.status == 'active'`. Votes after closure are rejected.
- **Empty agenda:** Normal state. Room sits idle. `GET /rooms/capacity` reports it.
- **No cadence configured:** Rooms without `poll_duration_secs` are unmanaged (current manual behavior preserved).
- **Duplicate/stale messages:** All handlers check current state before acting (idempotent).
- **Consumer failure:** Unacknowledged pgmq messages become visible after visibility timeout (automatic retry).

## Future Extension: Controversy-Based Duration

The design naturally supports extending poll duration for controversial topics:

- `compute_poll_stats()` already returns stddev per dimension (controversy signal)
- When `ClosePoll` fires, the handler can check: "is stddev above threshold?"
- If yes: update `closes_at += extension`, re-enqueue `ClosePoll`, skip close
- If no: close normally
- Estimated cost to add: ~1 day. No schema changes needed.

## Testing Strategy

- **Unit tests:** Lifecycle service methods with mock repo — verify close → activate-next → enqueue flow
- **Integration tests:** Full pgmq cycle with testcontainers Postgres — create room with cadence, push polls, verify auto-rotation
- **Sim integration:** Verify sim uses capacity endpoint and doesn't manually activate polls
- **Edge cases:** Empty agenda, missing cadence, concurrent close attempts

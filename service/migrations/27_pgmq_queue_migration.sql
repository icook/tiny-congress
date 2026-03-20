-- Migration 27: Migrate homegrown queues to pgmq
--
-- Creates pgmq queues for lifecycle and trust actions, renames
-- trust__action_queue to trust__action_log, and drops the old
-- lifecycle queue table.

-- 1. Create the two new pgmq queues
SELECT pgmq.create('rooms__lifecycle');
SELECT pgmq.create('trust__actions');

-- 2. Migrate any in-flight lifecycle messages to the pgmq queue.
--    We insert directly into the pgmq queue table, setting vt (visibility time)
--    to the original visible_at timestamp so delayed messages stay delayed.
INSERT INTO pgmq.q_rooms__lifecycle (vt, message)
SELECT
    GREATEST(visible_at, now()),
    payload
FROM rooms__lifecycle_queue
ORDER BY id;

-- 3. Drop the old lifecycle queue table (data migrated above)
DROP TABLE IF EXISTS rooms__lifecycle_queue;

-- 4. Rename trust__action_queue → trust__action_log
--    In-flight 'pending' rows stay in the table; the worker will drain them on startup.
ALTER TABLE trust__action_queue RENAME TO trust__action_log;

-- 5. Rename indexes to match new table name
ALTER INDEX IF EXISTS idx_action_queue_pending
    RENAME TO idx_action_log_pending;
ALTER INDEX IF EXISTS idx_action_queue_actor_date
    RENAME TO idx_action_log_actor_date;

-- Migration 27: Migrate homegrown queues to pgmq
--
-- Creates pgmq queues for lifecycle and trust actions, renames
-- trust__action_queue to trust__action_log, and drops the old
-- lifecycle queue table.
--
-- All statements are idempotent so the migration can be re-run safely.

-- 1. Create the two new pgmq queues (pgmq.create is not idempotent, so guard)
DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pgmq.meta WHERE queue_name = 'rooms__lifecycle') THEN
        PERFORM pgmq.create('rooms__lifecycle');
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pgmq.meta WHERE queue_name = 'trust__actions') THEN
        PERFORM pgmq.create('trust__actions');
    END IF;
END $$;

-- 2. Migrate any in-flight lifecycle messages to the pgmq queue.
--    Only runs if the old table still exists (first run).
DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables
               WHERE table_name = 'rooms__lifecycle_queue') THEN
        INSERT INTO pgmq.q_rooms__lifecycle (vt, message)
        SELECT GREATEST(visible_at, now()), payload
        FROM rooms__lifecycle_queue
        ORDER BY id;
    END IF;
END $$;

-- 3. Drop the old lifecycle queue table (data migrated above)
DROP TABLE IF EXISTS rooms__lifecycle_queue;

-- 4. Rename trust__action_queue → trust__action_log
--    If both exist (idempotent re-run), drop the stale source table.
--    If only the old name exists (first run), rename it.
DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables
               WHERE table_name = 'trust__action_log') THEN
        -- Already migrated; drop the re-created source if present
        DROP TABLE IF EXISTS trust__action_queue;
    ELSIF EXISTS (SELECT 1 FROM information_schema.tables
                  WHERE table_name = 'trust__action_queue') THEN
        ALTER TABLE trust__action_queue RENAME TO trust__action_log;
    END IF;
END $$;

-- 5. Rename indexes to match new table name
ALTER INDEX IF EXISTS idx_action_queue_pending
    RENAME TO idx_action_log_pending;
ALTER INDEX IF EXISTS idx_action_queue_actor_date
    RENAME TO idx_action_log_actor_date;

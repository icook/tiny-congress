-- Room lifecycle: cadence config + poll scheduling + message queue

-- Room-level rotation cadence (NULL = manual-only, no auto-rotation)
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'rooms__rooms' AND column_name = 'poll_duration_secs'
    ) THEN
        ALTER TABLE rooms__rooms ADD COLUMN poll_duration_secs INTEGER;
    END IF;
END $$;

-- Poll scheduling within a room's agenda
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'rooms__polls' AND column_name = 'closes_at'
    ) THEN
        ALTER TABLE rooms__polls ADD COLUMN closes_at TIMESTAMPTZ;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'rooms__polls' AND column_name = 'agenda_position'
    ) THEN
        ALTER TABLE rooms__polls ADD COLUMN agenda_position INTEGER;
    END IF;
END $$;

-- Lightweight lifecycle message queue (pgmq semantics via plain SQL)
-- Messages become visible at visible_at; consumed via FOR UPDATE SKIP LOCKED.
CREATE TABLE IF NOT EXISTS rooms__lifecycle_queue (
    id BIGSERIAL PRIMARY KEY,
    message_type TEXT NOT NULL CHECK (message_type IN ('close_poll', 'activate_next')),
    payload JSONB NOT NULL,
    visible_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_lifecycle_queue_visible
    ON rooms__lifecycle_queue (visible_at, id);

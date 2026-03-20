CREATE TABLE IF NOT EXISTS rooms__research_suggestions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    room_id UUID NOT NULL REFERENCES rooms__rooms(id),
    account_id UUID NOT NULL REFERENCES accounts(id),
    suggestion_text TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued',
    filter_reason TEXT,
    evidence_ids UUID[] NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    processed_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_suggestions_room_status
    ON rooms__research_suggestions(room_id, status, created_at);

CREATE INDEX IF NOT EXISTS idx_suggestions_account_daily
    ON rooms__research_suggestions(room_id, account_id, created_at);

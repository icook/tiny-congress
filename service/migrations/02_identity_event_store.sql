-- Identity sigchain event store
CREATE TABLE IF NOT EXISTS signed_events (
    event_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL,
    seqno BIGINT NOT NULL CHECK (seqno > 0),
    event_type TEXT NOT NULL,
    canonical_bytes_hash BYTEA NOT NULL,
    envelope_json JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_signed_events_account_seqno
    ON signed_events(account_id, seqno);

-- Speeds tail lookups per account
CREATE INDEX IF NOT EXISTS idx_signed_events_account_seqno_desc
    ON signed_events(account_id, seqno DESC);

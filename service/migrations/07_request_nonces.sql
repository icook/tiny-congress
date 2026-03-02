-- Replay protection: track recently-seen request nonces.
-- Rows are ephemeral — a background task deletes entries older than
-- the timestamp skew window (currently 300 seconds).

CREATE TABLE IF NOT EXISTS request_nonces (
    nonce_hash  BYTEA PRIMARY KEY,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Index for efficient cleanup of expired nonces.
CREATE INDEX IF NOT EXISTS idx_request_nonces_created_at
    ON request_nonces (created_at);

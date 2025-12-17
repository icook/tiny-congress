-- Rate limit tables for identity abuse controls
CREATE TABLE IF NOT EXISTS endorsement_rate_limits (
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    subject_type TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    topic TEXT NOT NULL DEFAULT '',
    window_start TIMESTAMPTZ NOT NULL,
    count INT NOT NULL DEFAULT 0,
    PRIMARY KEY (account_id, subject_type, subject_id, topic, window_start)
);

CREATE INDEX IF NOT EXISTS idx_endorsement_rate_limits_window
    ON endorsement_rate_limits(account_id, window_start);

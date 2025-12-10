-- Core identity read models
CREATE TABLE IF NOT EXISTS accounts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    root_kid TEXT NOT NULL,
    root_pubkey TEXT NOT NULL,
    tier TEXT NOT NULL DEFAULT 'anonymous' CHECK (tier IN ('anonymous', 'verified', 'bonded', 'vouched')),
    verification_state TEXT NOT NULL DEFAULT 'none' CHECK (verification_state IN ('none', 'pending', 'verified', 'rejected')),
    bond_state TEXT NOT NULL DEFAULT 'none' CHECK (bond_state IN ('none', 'deposited', 'withdrawn')),
    profile_json JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE TABLE IF NOT EXISTS devices (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    device_kid TEXT NOT NULL,
    device_pubkey TEXT NOT NULL,
    name TEXT,
    type TEXT NOT NULL DEFAULT 'other' CHECK (type IN ('phone', 'laptop', 'tablet', 'hardware', 'other')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    revocation_reason TEXT,
    CONSTRAINT unique_device_kid_per_account UNIQUE (account_id, device_kid)
);

CREATE TABLE IF NOT EXISTS device_delegations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    delegation_envelope JSONB NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    CONSTRAINT fk_device_account FOREIGN KEY (account_id, device_id) REFERENCES devices(account_id, id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_device_delegations_active
    ON device_delegations(account_id, device_id)
    WHERE revoked_at IS NULL;

CREATE TABLE IF NOT EXISTS endorsements (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    author_account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    author_device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    subject_type TEXT NOT NULL CHECK (subject_type IN ('account', 'root_kid', 'device_kid', 'url', 'org', 'statement_hash')),
    subject_id TEXT NOT NULL,
    topic TEXT NOT NULL,
    magnitude DOUBLE PRECISION NOT NULL CHECK (magnitude >= -1.0 AND magnitude <= 1.0),
    confidence DOUBLE PRECISION NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    context TEXT,
    tags TEXT[],
    evidence_url TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked_at TIMESTAMPTZ,
    envelope JSONB NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_endorsements_subject_topic
    ON endorsements(subject_type, subject_id, topic)
    WHERE revoked_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_endorsements_tags
    ON endorsements USING GIN (tags);

CREATE TABLE IF NOT EXISTS endorsement_aggregates (
    subject_type TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    topic TEXT NOT NULL,
    n_total INT NOT NULL DEFAULT 0,
    n_pos INT NOT NULL DEFAULT 0,
    n_neg INT NOT NULL DEFAULT 0,
    sum_weight DOUBLE PRECISION NOT NULL DEFAULT 0,
    weighted_mean DOUBLE PRECISION,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (subject_type, subject_id, topic)
);

CREATE TABLE IF NOT EXISTS recovery_policies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    threshold INT NOT NULL CHECK (threshold > 0),
    helpers JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked_at TIMESTAMPTZ,
    envelope JSONB NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_active_recovery_policy
    ON recovery_policies(account_id)
    WHERE revoked_at IS NULL;

CREATE TABLE IF NOT EXISTS recovery_approvals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    policy_id UUID NOT NULL REFERENCES recovery_policies(id) ON DELETE CASCADE,
    new_root_kid TEXT NOT NULL,
    new_root_pubkey TEXT NOT NULL,
    helper_account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    helper_device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    envelope JSONB NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_recovery_approvals_policy
    ON recovery_approvals(account_id, policy_id);

CREATE TABLE IF NOT EXISTS sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    issued_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    scopes TEXT[] NOT NULL DEFAULT '{}',
    auth_factors JSONB NOT NULL,
    challenge_nonce TEXT,
    challenge_expires_at TIMESTAMPTZ,
    used_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_sessions_account_device
    ON sessions(account_id, device_id, expires_at);

CREATE TABLE IF NOT EXISTS reputation_scores (
    account_id UUID PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
    score DOUBLE PRECISION NOT NULL DEFAULT 0,
    posture_label TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Add FK now that accounts exists
ALTER TABLE signed_events
    ADD CONSTRAINT signed_events_account_fk
    FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE;

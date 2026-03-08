-- Migration 13: Trust engine supporting tables
-- Creates influence ledger, action queue, score snapshots, denouncements, and invites

-- 1. Per-user influence ledger
CREATE TABLE IF NOT EXISTS trust__user_influence (
    user_id UUID PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
    total_influence REAL NOT NULL DEFAULT 10.0,
    staked_influence REAL NOT NULL DEFAULT 0,
    spent_influence REAL NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (staked_influence >= 0),
    CHECK (spent_influence >= 0),
    CHECK (total_influence >= 0)
);

-- 2. Async action queue for trust operations
CREATE TABLE IF NOT EXISTS trust__action_queue (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    action_type TEXT NOT NULL CHECK (action_type IN ('endorse', 'revoke', 'denounce')),
    payload JSONB NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'processing', 'completed', 'failed')),
    quota_date DATE NOT NULL DEFAULT CURRENT_DATE,
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    processed_at TIMESTAMPTZ
);
-- Index on pending status for efficient worker polling
CREATE INDEX IF NOT EXISTS idx_action_queue_pending
    ON trust__action_queue(status) WHERE status = 'pending';
-- Index for daily quota counting
CREATE INDEX IF NOT EXISTS idx_action_queue_actor_date
    ON trust__action_queue(actor_id, quota_date);

-- 3. Trust score snapshots (per-user, optionally per-context)
CREATE TABLE IF NOT EXISTS trust__score_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    context_user_id UUID REFERENCES accounts(id) ON DELETE CASCADE,
    trust_distance REAL,
    path_diversity INTEGER,
    eigenvector_centrality REAL,
    computed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
-- Unique: one score per (user, context) pair
CREATE UNIQUE INDEX IF NOT EXISTS uq_score_snapshot_user_context
    ON trust__score_snapshots(user_id, context_user_id);
-- Separate partial index for global scores (context_user_id IS NULL)
-- because PostgreSQL treats NULLs as distinct in UNIQUE indexes
CREATE UNIQUE INDEX IF NOT EXISTS uq_score_snapshot_user_global
    ON trust__score_snapshots(user_id) WHERE context_user_id IS NULL;

-- 4. Denouncements (negative trust signals)
CREATE TABLE IF NOT EXISTS trust__denouncements (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    accuser_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    target_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    reason TEXT NOT NULL,
    influence_spent REAL NOT NULL CHECK (influence_spent > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at TIMESTAMPTZ,
    CONSTRAINT uq_denouncement_accuser_target UNIQUE (accuser_id, target_id),
    CONSTRAINT chk_denouncement_not_self CHECK (accuser_id != target_id)
);

-- 5. Invite envelopes (used during onboarding via endorser)
CREATE TABLE IF NOT EXISTS trust__invites (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    endorser_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    envelope BYTEA NOT NULL,
    delivery_method TEXT NOT NULL CHECK (delivery_method IN ('qr', 'email')),
    attestation JSONB NOT NULL,
    accepted_by UUID REFERENCES accounts(id),
    expires_at TIMESTAMPTZ NOT NULL,
    accepted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_invites_endorser
    ON trust__invites(endorser_id);

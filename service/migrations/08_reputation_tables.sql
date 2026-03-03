-- Reputation module tables for the endorsement system.
-- Endorsements gate room participation: a verified user holds an endorsement
-- for a topic (e.g., "identity_verified") issued by a verifier service account.

-- Service accounts authorized to issue endorsements (e.g., the ID.me verifier).
CREATE TABLE IF NOT EXISTS reputation__verifier_accounts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_verifier_accounts_name UNIQUE (name)
);

-- An endorsement asserts that a subject (account) has a particular quality (topic).
-- One active endorsement per (subject, topic). Revocable via revoked_at.
CREATE TABLE IF NOT EXISTS reputation__endorsements (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    subject_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    topic TEXT NOT NULL,
    issuer_id UUID NOT NULL REFERENCES reputation__verifier_accounts(id),
    evidence JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at TIMESTAMPTZ,
    CONSTRAINT uq_endorsements_subject_topic UNIQUE (subject_id, topic)
);

CREATE INDEX IF NOT EXISTS idx_endorsements_subject ON reputation__endorsements(subject_id);
CREATE INDEX IF NOT EXISTS idx_endorsements_topic ON reputation__endorsements(topic);

-- Links external provider identities to TC accounts (sybil prevention).
-- One link per (provider, provider_subject) ensures the same external identity
-- cannot be linked to multiple TC accounts.
CREATE TABLE IF NOT EXISTS reputation__external_identities (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    provider_subject TEXT NOT NULL,
    linked_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_external_identities_provider_subject UNIQUE (provider, provider_subject)
);

CREATE INDEX IF NOT EXISTS idx_external_identities_account ON reputation__external_identities(account_id);

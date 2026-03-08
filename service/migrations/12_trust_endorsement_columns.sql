-- Migration: Add trust/weight columns to endorsements and rename issuer_id → endorser_id.
-- Idempotent: all operations guarded with IF EXISTS / IF NOT EXISTS / information_schema checks.

-- 1. Rename issuer_id → endorser_id if the old column name still exists.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'reputation__endorsements'
          AND column_name = 'issuer_id'
    ) THEN
        ALTER TABLE reputation__endorsements
            RENAME COLUMN issuer_id TO endorser_id;
    END IF;
END $$;

-- 2. Rename FK constraint fk_endorsements_issuer → fk_endorsements_endorser if old name exists.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fk_endorsements_issuer'
    ) THEN
        ALTER TABLE reputation__endorsements
            RENAME CONSTRAINT fk_endorsements_issuer TO fk_endorsements_endorser;
    END IF;
END $$;

-- 3. Drop old unique index on issuer_id (if it still exists under the old name).
-- Note: uq_endorsements_genesis (partial index WHERE issuer_id IS NULL, from migration 10)
-- is intentionally retained. PostgreSQL automatically updates its definition when the
-- column is renamed to endorser_id, so it continues to enforce genesis uniqueness correctly.
DROP INDEX IF EXISTS uq_endorsements_subject_topic_issuer;

-- 4. Create unique index on endorser_id if it doesn't exist yet.
CREATE UNIQUE INDEX IF NOT EXISTS uq_endorsements_subject_topic_endorser
    ON reputation__endorsements (subject_id, topic, endorser_id);

-- 5. Add new columns if they don't already exist.
ALTER TABLE reputation__endorsements
    ADD COLUMN IF NOT EXISTS weight REAL NOT NULL DEFAULT 1.0
        CHECK (weight > 0 AND weight <= 1.0);

ALTER TABLE reputation__endorsements
    ADD COLUMN IF NOT EXISTS influence_staked REAL NOT NULL DEFAULT 0;

ALTER TABLE reputation__endorsements
    ADD COLUMN IF NOT EXISTS attestation JSONB;

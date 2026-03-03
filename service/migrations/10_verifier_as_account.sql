-- Migration: Convert verifier accounts from separate entity to regular accounts.
-- Endorsement issuer_id changes from reputation__verifier_accounts(id) to accounts(id).
-- Existing endorsements become genesis (NULL issuer) since old verifier accounts
-- have no corresponding user account.

-- 1. Drop FK from issuer_id to reputation__verifier_accounts (if it exists)
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'reputation__endorsements_issuer_id_fkey'
    ) THEN
        ALTER TABLE reputation__endorsements
            DROP CONSTRAINT reputation__endorsements_issuer_id_fkey;
    END IF;
END $$;

-- 2. Nullify existing issuer_ids (old verifier account UUIDs have no account mapping)
-- Safe to re-run: sets already-NULL values to NULL (no-op)
UPDATE reputation__endorsements SET issuer_id = NULL WHERE issuer_id IS NOT NULL;

-- 3. Allow NULL issuer (genesis endorsements)
-- ALTER COLUMN DROP NOT NULL is idempotent in PostgreSQL
ALTER TABLE reputation__endorsements
    ALTER COLUMN issuer_id DROP NOT NULL;

-- 4. Add FK to accounts(id) for non-NULL issuers (if not already present)
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fk_endorsements_issuer'
    ) THEN
        ALTER TABLE reputation__endorsements
            ADD CONSTRAINT fk_endorsements_issuer
            FOREIGN KEY (issuer_id) REFERENCES accounts(id);
    END IF;
END $$;

-- 5. Replace old unique constraint with wider one (if it still exists)
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'uq_endorsements_subject_topic'
    ) THEN
        ALTER TABLE reputation__endorsements
            DROP CONSTRAINT uq_endorsements_subject_topic;
    END IF;
END $$;

-- Multiple verifiers can endorse the same (subject, topic)
CREATE UNIQUE INDEX IF NOT EXISTS uq_endorsements_subject_topic_issuer
    ON reputation__endorsements (subject_id, topic, issuer_id);

-- Prevent duplicate genesis endorsements (PostgreSQL treats NULLs as distinct in UNIQUE)
CREATE UNIQUE INDEX IF NOT EXISTS uq_endorsements_genesis
    ON reputation__endorsements (subject_id, topic) WHERE issuer_id IS NULL;

-- 6. Drop the old verifier accounts table (no longer needed)
DROP TABLE IF EXISTS reputation__verifier_accounts;

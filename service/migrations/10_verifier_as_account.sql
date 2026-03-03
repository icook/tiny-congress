-- Migration: Convert verifier accounts from separate entity to regular accounts.
-- Endorsement issuer_id changes from reputation__verifier_accounts(id) to accounts(id).
-- Existing endorsements become genesis (NULL issuer) since old verifier accounts
-- have no corresponding user account.

-- 1. Drop FK from issuer_id to reputation__verifier_accounts
ALTER TABLE reputation__endorsements
    DROP CONSTRAINT reputation__endorsements_issuer_id_fkey;

-- 2. Nullify existing issuer_ids (old verifier account UUIDs have no account mapping)
UPDATE reputation__endorsements SET issuer_id = NULL;

-- 3. Allow NULL issuer (genesis endorsements)
ALTER TABLE reputation__endorsements
    ALTER COLUMN issuer_id DROP NOT NULL;

-- 4. Add FK to accounts(id) for non-NULL issuers
ALTER TABLE reputation__endorsements
    ADD CONSTRAINT fk_endorsements_issuer
    FOREIGN KEY (issuer_id) REFERENCES accounts(id);

-- 5. Replace old unique constraint with wider one
ALTER TABLE reputation__endorsements
    DROP CONSTRAINT uq_endorsements_subject_topic;

-- Multiple verifiers can endorse the same (subject, topic)
CREATE UNIQUE INDEX uq_endorsements_subject_topic_issuer
    ON reputation__endorsements (subject_id, topic, issuer_id);

-- Prevent duplicate genesis endorsements (PostgreSQL treats NULLs as distinct in UNIQUE)
CREATE UNIQUE INDEX uq_endorsements_genesis
    ON reputation__endorsements (subject_id, topic) WHERE issuer_id IS NULL;

-- 6. Drop the old verifier accounts table (no longer needed)
DROP TABLE reputation__verifier_accounts;

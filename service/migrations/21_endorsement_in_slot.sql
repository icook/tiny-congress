-- Add in_slot flag to endorsements. Existing rows default to true (all current endorsements are in-slot).
-- Out-of-slot endorsements are stored but excluded from trust graph computation.
ALTER TABLE reputation__endorsements ADD COLUMN IF NOT EXISTS in_slot BOOLEAN NOT NULL DEFAULT true;

-- Index for efficient budget counting: only count in-slot endorsements.
CREATE INDEX IF NOT EXISTS idx_endorsements_in_slot_budget
    ON reputation__endorsements (endorser_id)
    WHERE topic = 'trust' AND revoked_at IS NULL AND in_slot = true;

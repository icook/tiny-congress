-- Migration: Drop influence_staked column from reputation__endorsements.
-- This column was added in migration 12 to track influence locked per endorsement,
-- but the slot-based model (ADR-020) derives capacity from a live count query instead.
-- The column was never populated by application code (defaulted to 0).

ALTER TABLE reputation__endorsements
    DROP COLUMN IF EXISTS influence_staked;

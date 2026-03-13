-- Migration: Drop influence_spent column from trust__denouncements.
-- ADR-020 replaces the continuous influence pool with discrete slots.
-- Denouncements now have a permanent budget of d=2 per user and do not
-- deduct from the influence pool. The column was always set by application
-- code only; no external consumers depend on it.

ALTER TABLE trust__denouncements
    DROP COLUMN IF EXISTS influence_spent;

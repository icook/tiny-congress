-- Add structured constraint columns to rooms__rooms.
-- Replaces the string-only eligibility_topic with a typed constraint_type
-- and a JSONB config blob. The eligibility_topic column is preserved for
-- backward compatibility and to avoid a destructive migration.

ALTER TABLE rooms__rooms
    ADD COLUMN IF NOT EXISTS constraint_type TEXT,
    ADD COLUMN IF NOT EXISTS constraint_config JSONB;

-- Migrate existing rows: map eligibility_topic → endorsed_by constraint.
-- WHERE constraint_type IS NULL makes this naturally idempotent.
UPDATE rooms__rooms
SET
    constraint_type   = 'endorsed_by',
    constraint_config = jsonb_build_object('topic', eligibility_topic)
WHERE constraint_type IS NULL;

-- Set default for future inserts after migration is complete.
ALTER TABLE rooms__rooms
    ALTER COLUMN constraint_type SET DEFAULT 'endorsed_by';

-- Migration 18: Add weight and relationship_depth to trust__invites,
-- and expand delivery_method to include video/text/messaging channels.

-- Drop the old check constraint by name (Postgres names it automatically;
-- look it up or drop by recreating — alter table approach)
ALTER TABLE trust__invites
    DROP CONSTRAINT IF EXISTS trust__invites_delivery_method_check;

ALTER TABLE trust__invites
    ADD CONSTRAINT trust__invites_delivery_method_check
        CHECK (delivery_method IN ('qr', 'email', 'video', 'text', 'messaging'));

ALTER TABLE trust__invites
    ADD COLUMN IF NOT EXISTS relationship_depth TEXT
        CHECK (relationship_depth IN ('years', 'months', 'acquaintance')),
    ADD COLUMN IF NOT EXISTS weight FLOAT NOT NULL DEFAULT 1.0;

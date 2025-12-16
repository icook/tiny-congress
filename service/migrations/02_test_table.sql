-- Simple test table to verify DB integration tests work
-- This can be removed or replaced when real tables are added

CREATE TABLE IF NOT EXISTS test_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

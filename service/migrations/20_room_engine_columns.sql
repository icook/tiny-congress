-- Add engine_type and engine_config to rooms table.
-- All existing rooms are polling rooms.
ALTER TABLE rooms__rooms
  ADD COLUMN engine_type TEXT NOT NULL DEFAULT 'polling',
  ADD COLUMN engine_config JSONB NOT NULL DEFAULT '{}';

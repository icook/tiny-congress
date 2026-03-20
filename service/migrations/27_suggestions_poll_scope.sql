DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_name = 'rooms__research_suggestions' AND column_name = 'poll_id'
  ) THEN
    ALTER TABLE rooms__research_suggestions
      ADD COLUMN poll_id UUID NOT NULL REFERENCES rooms__polls(id);
  END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_suggestions_poll_status
  ON rooms__research_suggestions(poll_id, status, created_at);

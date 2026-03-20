ALTER TABLE rooms__research_suggestions
  ADD COLUMN poll_id UUID NOT NULL REFERENCES rooms__poll_polls(id);

CREATE INDEX IF NOT EXISTS idx_suggestions_poll_status
  ON rooms__research_suggestions(poll_id, status, created_at);

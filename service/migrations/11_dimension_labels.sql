-- Add optional human-readable labels for dimension slider endpoints.
ALTER TABLE rooms__poll_dimensions
    ADD COLUMN IF NOT EXISTS min_label TEXT;
ALTER TABLE rooms__poll_dimensions
    ADD COLUMN IF NOT EXISTS max_label TEXT;

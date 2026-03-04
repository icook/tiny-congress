-- Add optional human-readable labels for dimension slider endpoints.
ALTER TABLE rooms__poll_dimensions
    ADD COLUMN min_label TEXT,
    ADD COLUMN max_label TEXT;

-- 19_poll_evidence.sql
CREATE TABLE IF NOT EXISTS rooms__poll_evidence (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    dimension_id UUID NOT NULL REFERENCES rooms__poll_dimensions(id) ON DELETE CASCADE,
    stance       TEXT NOT NULL CHECK (stance IN ('pro', 'con')),
    claim        TEXT NOT NULL,
    source       TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_poll_evidence_dimension ON rooms__poll_evidence(dimension_id);

-- Add missing indexes for endorser_id and target_id.
-- The trust engine CTE filters WHERE e.endorser_id = $1 AND e.topic = 'trust';
-- the existing unique index leads with subject_id and cannot serve that query.
-- The denouncements unique constraint on (accuser_id, target_id) cannot serve
-- queries filtering by target_id alone.
CREATE INDEX idx_endorsements_endorser ON reputation__endorsements(endorser_id);
CREATE INDEX idx_denouncements_target ON trust__denouncements(target_id);

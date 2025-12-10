# BE-08 Aggregation reducer

Goal: materialize per-subject/topic endorsement aggregates and derived reputation/security posture scores, all replayable from signed_events.

Deliverables
- Reducer that rebuilds aggregates deterministically from event log.
- Tables/indexes for aggregates and posture summary.
- Read endpoints for reputation and posture views.

Implementation plan (service)
1) Schema: add `endorsement_aggregates` table with columns `(subject_type TEXT, subject_id TEXT, topic TEXT, n_total INT, n_pos INT, n_neg INT, sum_weight DOUBLE PRECISION, weighted_mean DOUBLE PRECISION, updated_at TIMESTAMPTZ)`, primary key `(subject_type, subject_id, topic)`. Add `reputation_scores(account_id UUID PRIMARY KEY, score DOUBLE PRECISION, posture_label TEXT, updated_at TIMESTAMPTZ)` to store latest derived values. Consider a `device_activity` view for posture inputs.

2) Reducer: create `service/src/identity/reducer/aggregates.rs` with pure functions that take an iterator of endorsement events and compute aggregates. Hook into append flow (BE-07) to update aggregates transactionally. Provide a `replay_all(pool)` helper that clears aggregates and recomputes from `signed_events` for audits.

3) Reputation heuristic: define helper `compute_reputation(account_row, aggregates, posture)` that follows v0 spec (tier bonus, endorsement_bonus from trustworthy/is_real_person, security_bonus from posture). Store output in `reputation_scores` table; avoid locking main tables by doing it in same transaction as aggregate update.

4) Read API: add endpoints `GET /users/:id/reputation` and `GET /users/:id/security_posture` in `http/profile.rs` that read from `reputation_scores` and posture summaries (active device count, last seen, factor counts).

5) Tests: unit tests for aggregate math (weighted mean) and reputation inputs. Integration test: create endorsements with varying magnitude/confidence, ensure aggregates and reputation_score reflect updates; revocation should decrement counts and recalc.

Verification
- `cd service && cargo test identity_aggregates`.
- `skaffold test -p ci` to ensure replay path works inside container images.
- Manual replay: run a CLI task that wipes aggregates, replays from signed_events, and compare to live tables (should match).

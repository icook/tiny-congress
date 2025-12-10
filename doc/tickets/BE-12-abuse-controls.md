# BE-12 Abuse controls

Goal: basic anti-abuse for endorsement writes and auth attempts, plus audit logging.

Deliverables
- Rate limiting on endorsement creation per author and per subject.
- Audit logs for auth failures and endorsement writes.
- Rejection paths for spammy behavior surfaced via metrics/logs.

Implementation plan (service)
1) Rate limiting:
   - Implement middleware or helper in `service/src/identity/http/middleware.rs` using a simple in-DB counter or Redis (if available). For now, use Postgres: table `endorsement_rate_limits(account_id UUID, window_start TIMESTAMPTZ, count INT)` with upsert to enforce e.g., 50 endorsements/day per account and 10 per subject/topic/day. Add indexes on `(account_id, window_start)` and `(subject_id, window_start)`.
   - Apply check in `POST /endorsements` before append.

2) Abuse heuristics: add check to block magnitude/confidence combos that look spammy (e.g., same subject/topic repeated). Emit warning logs with account_id.

3) Audit logs: use `tracing` to emit structured events (`auth.failure`, `endorsement.write`, `endorsement.rate_limited`) with account_id/device_id/subject/topic. Consider a simple `audit_log` table if persistence is needed.

4) Tests: unit test rate limiter helper with simulated time (use `time::OffsetDateTime` or inject clock). Integration test hitting endorsement endpoint repeatedly to trigger 429 and ensure count resets after window.

Verification
- `cd service && cargo test identity_abuse_controls`.
- `skaffold test -p ci` to ensure limits don't break container runs.

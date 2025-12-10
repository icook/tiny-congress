# INF-03 Observability

Goal: structured logs and metrics for auth, signature failures, endorsements, and reducers. Provide visibility in local and CI runs.

Deliverables
- Tracing instrumentation around auth flows, sigchain append, and reducer replays.
- Metrics exports (e.g., Prometheus) for login success/failure, revoked device attempts, endorsement writes.
- Logging for abuse/rate-limit events.

Implementation plan
1) Logging: use existing `tracing` setup in `service/src/main.rs`. Add spans in identity handlers (`auth`, `devices`, `endorsements`, `recovery`) including account_id/device_id where safe. Log signature verification failures with reason (non-canonical, bad signature, revoked device) at warn level.

2) Metrics: add `metrics` crate + Prometheus exporter (e.g., `metrics-exporter-prometheus`) initialized in `main.rs`. Emit counters `auth.success`, `auth.failure`, `device.revoked_attempt`, `endorsement.write`, `endorsement.revocation`, `reducer.replay_seconds` (histogram).

3) Health/probes: extend existing `/health` if needed to include DB connectivity status (simple query). Add `/metrics` endpoint guarded if necessary.

4) CI/Dev: document in `service/README.md` how to scrape `/metrics` locally (curl) and how to enable debug logging via `RUST_LOG` env. Ensure Skaffold profile exposes metrics port.

5) Tests: add a small integration test asserting metric counters increment (use in-memory recorder). For logging, rely on manual inspection; ensure tests don’t fail due to missing exporter by gating with feature flag.

Verification
- `cd service && cargo test identity_observability`.
- Manual: run server, hit auth endpoints, curl `/metrics`, confirm counters increment.

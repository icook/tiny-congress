# reset demo db

Reset the demo environment database and re-run the simulation to get fresh seed data.

Use this when demo data is stale (missing fields from new migrations, constraint config mismatches, or the sim CronJob is failing).

## Steps

### 1. Truncate all app data

Exec into the demo postgres pod and truncate the root tables. CASCADE handles all FK dependencies (votes, polls, endorsements, device_keys, trust scores, etc.):

```bash
kubectl -n tiny-congress-demo exec deployment/tc-demo-postgres -c postgres -- \
  psql -U postgres -d tiny-congress \
  -c "TRUNCATE accounts, rooms__rooms, request_nonces, rooms__lifecycle_queue CASCADE;"
```

Confirm the output shows `TRUNCATE TABLE` with cascade notices for dependent tables.

### 2. Restart the API to re-bootstrap verifiers

The API's `reputation/bootstrap.rs` creates verifier accounts (`sim_verifier`, `demo_verifier`) and their genesis `authorized_verifier` endorsements on startup:

```bash
kubectl -n tiny-congress-demo rollout restart deployment/tc-demo
kubectl -n tiny-congress-demo rollout status deployment/tc-demo --timeout=120s
```

Wait for the rollout to complete before proceeding.

### 3. Trigger the sim manually

Don't wait for the 30-minute CronJob schedule:

```bash
JOB_NAME="tc-demo-sim-reseed-$(date +%s)"
kubectl -n tiny-congress-demo create job --from=cronjob/tc-demo-sim "$JOB_NAME"
```

Tail the logs to confirm success:

```bash
kubectl -n tiny-congress-demo logs -f "job/$JOB_NAME"
```

### 4. Verify

Confirm the sim logs show:
- `rooms_created` > 0 (or `room target met` if rooms already exist)
- `votes_cast` > 0
- `tc-sim run complete`
- No `ERROR` lines (especially no `anchor_id` or `constraint` errors)

If the sim fails, check the API logs for errors:

```bash
kubectl -n tiny-congress-demo logs deployment/tc-demo --tail=50 | grep -i error
```

Or query Loki via Grafana MCP:

```
{namespace="tiny-congress-demo",pod=~"tc-demo-5.*"} |~ "ERROR|error"
```

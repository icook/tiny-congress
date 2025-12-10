# BE-09 Recovery policy

Goal: allow accounts to author a root-signed recovery policy listing helpers and threshold; store as sigchain link and materialize current policy.

Deliverables
- Endpoint to create/update recovery policy with root signature.
- Reducer to persist policy and revocations in `recovery_policies` table.
- Validation of helper list and threshold.

Implementation plan (service)
1) Schema: `recovery_policies` table from INF-01 with columns `(id UUID PK, account_id UUID FK, threshold INT, helpers JSONB, created_at, revoked_at, envelope JSONB)`. Add unique partial index on `(account_id)` where `revoked_at IS NULL`.

2) Handler: `POST /me/recovery_policy` in `service/src/identity/http/recovery.rs`.
   - Require authenticated session.
   - Validate threshold >=1 and <= helpers.len(). Helpers entries `{ helper_account_id, helper_root_kid? }`.
   - Verify envelope signature against current root key (BE-02) and seqno via BE-01.
   - Append `RecoveryPolicySet` event (or `RecoveryPolicyRevoked` for revocation endpoint).

3) Reducer: update `recovery_policies` on set (insert new row, revoke previous active policy by setting `revoked_at`). On revocation event, mark active policy revoked without creating a new one.

4) Exposure: `GET /me/recovery_policy` returns active policy plus helper statuses (from approvals table once BE-10 exists).

5) Tests: integration test creating a policy, fetching it, and revoking. Negative cases: threshold > helpers count (422), bad signature (400), seqno mismatch (409).

Verification
- `cd service && cargo test identity_recovery_policy`.
- `skaffold test -p ci` to apply migrations in CI.

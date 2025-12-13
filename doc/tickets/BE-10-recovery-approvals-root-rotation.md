# BE-10 Recovery approvals + root rotation

Goal: support helper approvals for recovery, validate threshold, and append a root rotation link that updates the active root key and invalidates delegations unless re-issued.

Deliverables
- Endpoints for helper approval and root rotation.
- Event append + reducers for `RecoveryApproval` and `RootRotation`.
- Logic to invalidate device delegations on rotation unless redelegated.

Implementation plan (service)
1) Schema: `recovery_approvals` table with `(id UUID PK, account_id, policy_id, new_root_kid, new_root_pubkey, helper_account_id, helper_device_id, envelope JSONB, created_at)`, FK to `recovery_policies`. Add index on `(account_id, policy_id)` for threshold checks.

2) Helper approval endpoint: `POST /recovery/approve` in `http/recovery.rs`.
   - Auth with session belonging to helper account/device.
   - Validate policy exists, not revoked, helper is listed, and no duplicate approval by same helper.
   - Verify envelope signature with helper device key (BE-02) and append `RecoveryApproval` event.

3) Root rotation endpoint: `POST /recovery/rotate_root` (server-side operation initiated after approvals collected).
   - Validate approvals meet threshold for active policy and all approvals target the same `new_root_kid/pubkey`.
   - Append `RootRotation` event signed by server? Prefer root key rotation envelope signed by existing root (if accessible) or mark as server-validated per spec. Include new root pubkey and update seqno.
   - Reducer updates `accounts.root_kid/root_pubkey`, clears/marks old delegations invalid, and optionally re-delegates devices if new delegations are included in the rotation payload.

4) Delegation invalidation: in reducer, set `device_delegations.revoked_at=rotation_time` for delegations tied to old root unless a matching redelegation exists in the rotation payload. Block logins for devices without an active delegation.

5) Tests: integration covering: create policy -> approvals from helpers -> rotate root -> verify account root_kid updated, old delegations revoked, and login must fail until new delegation exists. Add negative tests for insufficient approvals and mismatched new_root_kid.

Verification
- `cd service && cargo test identity_recovery_rotation`.
- `skaffold test -p ci` to ensure migrations survive container path.

# BE-05 Device revoke

Goal: allow root key to revoke a device, ensure revoked devices cannot authenticate, and mark delegation inactive in the read model.

Deliverables
- HTTP handler to accept a root-signed revocation link for a device.
- Sigchain append + reducer to set `devices.revoked_at`/`revocation_reason` and mark delegation inactive.
- Auth guard that blocks revoked devices during login/session issuance.

Implementation plan (service)
1) Request: `POST /me/devices/:id/revoke` with body `{ revocation_envelope }` where payload holds `device_id`, `reason`, `ctime`, `seqno`, `prev_hash`. Require session bound to account_id; root key must sign.

2) Handler: in `service/src/identity/http/devices.rs` add `revoke_device`.
   - Resolve account_id/device_id from path/body.
   - Verify envelope signature against current root key using BE-02; ensure seqno is next via BE-01.
   - Append `DeviceRevocation` event.

3) Reducer: in `reducer/devices.rs`, on `DeviceRevocation`, update `devices` row `revoked_at=now()`, `revocation_reason`, and invalidate delegation (set `revoked_at` on `device_delegations` or set `revoked` flag).

4) Auth guard: update BE-06 login verifier to read `devices.revoked_at IS NULL` and reject revoked device logins. Also block session issuance if `device_delegations.expires_at` < now.

5) Tests: integration test revoking a device then attempting login should fail. Ensure revocation is idempotent (second revoke returns 409/400). Confirm read model fields updated.

Verification
- `cd service && cargo test identity_device_revoke`.
- `skaffold test -p ci` after migrations that add `revoked_at` columns.

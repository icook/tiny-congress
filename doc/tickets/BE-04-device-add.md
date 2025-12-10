# BE-04 Device add flow

Goal: support adding a new device via a root-signed delegation link and materialize the device + delegation rows.

Deliverables
- HTTP handler to accept a DeviceDelegation for a new device and append to sigchain.
- Reducer updates `devices` and `device_delegations` tables (marking existing delegations intact).
- Tests for successful add and rejects (revoked root, bad signature, duplicate device).

Implementation plan (service)
1) Request shape: `POST /me/devices/add` (authenticated via existing session) with body `{ account_id, new_device_pubkey, device_metadata, delegation_envelope }`. `delegation_envelope` is root-signed and includes device_id, issued_at/ctime, optional expires_at.

2) Handler: `service/src/identity/http/devices.rs` adds `add_device`. Steps:
   - Authenticate session (reuse session middleware from BE-06 once available) to ensure the caller is the account owner.
   - Verify envelope signature with BE-02 crypto and ensure `signer.kid` matches current root_kid for the account.
   - Enforce uniqueness on `(account_id, device_id)` and `device_kid`.
   - Compute next seqno/prev_hash via BE-01 and append `DeviceDelegation` event.

3) Reducer: extend `service/src/identity/reducer/devices.rs` to insert into `devices` if new, and upsert `device_delegations` with envelope + issued_at/expires_at. Leave existing delegations untouched unless BE-10 root rotation invalidates them.

4) QR handoff path: record `device_metadata` (name/type) as part of the event body; include nonce from QR in the payload to bind the delegation to the request.

5) Tests: integration test posting a new device after creating an account. Validate DB rows and that a second submission with the same device_id returns 409. Negative test for signature mismatch (tamper envelope) returns 400.

Verification
- `cd service && cargo test identity_device_add`.
- `skaffold test -p ci` to ensure migrations keep indexes/uniques intact.

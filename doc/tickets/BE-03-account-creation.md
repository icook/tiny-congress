# BE-03 Account creation

Goal: allow creating an account with a root key and first device delegation, appending to the sigchain and materializing accounts/devices tables.

Deliverables
- HTTP handler for account creation (first device path) that validates payloads and signatures.
- Reducer logic to write `accounts`, `devices`, and `device_delegations` tables from the appended event.
- Tests for happy path plus bad signatures/duplicate usernames.

Implementation plan (service)
1) Schema: relies on INF-01 identity tables. Ensure `accounts(username UNIQUE, root_kid, root_pubkey, tier, verification_state, profile_json)` and `devices/device_delegations` exist with FK constraints to `accounts`.

2) Routing: create `service/src/identity/http/accounts.rs` with Axum handler `post_account_create`. Mount under `/auth/signup` (or `/accounts`) in `main.rs` via a new `identity_router()` inside `service/src/identity/http/mod.rs`. Keep GraphQL untouched.

3) Request/response shapes:
   - Request JSON: `{ username, root_pubkey, device_pubkey, device_metadata, delegation_envelope }` where `delegation_envelope` is root-signed DeviceDelegation (payload includes device_id/metadata/ctime/seqno/prev_hash).
   - Response: `{ account_id, device_id }` plus current root_kid.

4) Validation flow:
   - Use BE-02 crypto to canonicalize and verify the `delegation_envelope` signature against `root_pubkey`; ensure `signer.device_id` is blank/absent for root-signed link.
   - Enforce username uniqueness; normalize usernames (lowercase) before checking.
   - Build sigchain link with seqno=1 and prev_hash=null; call BE-01 append helper.

5) Reducer/write model:
   - Add `service/src/identity/reducer/accounts.rs` that listens to `AccountCreated` event (event_type). Inside the append transaction, call reducer to insert into `accounts`, `devices`, `device_delegations` (active delegation row).
   - Keep reducer pure: given event + transaction, execute inserts; no external state.

6) Seed data cleanup: remove `create_seed_data` coupling to topics for this path where relevant; the auth routes should not create demo topics. Keep existing seed logic for demo untouched but separate identity DB writes.

7) Tests:
   - Unit tests for handler with `axum::Router` using `tower::ServiceExt` similar to existing `api_tests.rs`; assert 201 + expected IDs.
   - DB integration test under `service/tests/identity_account_creation.rs`: start clean DB, POST signup, assert `accounts` row exists with root_kid == derived kid, `devices` row exists, delegation row has envelope JSON stored.
   - Negative tests: duplicate username 409, bad delegation signature 400, seqno mismatch 400.

Verification
- `cd service && cargo test identity_account_creation`.
- `skaffold test -p ci` once migrations are wired.
- Manual: curl the signup endpoint against running server and inspect DB with `psql`.

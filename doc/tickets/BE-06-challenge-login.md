# BE-06 Challenge-login

Goal: device-key login using challenge/response, with nonce tracking and session issuance tied to device_id.

Deliverables
- `POST /auth/challenge` and `POST /auth/verify` handlers.
- Nonce storage and invalidation (challenge table or reuse `sessions` with pending status).
- Session issuance backed by `sessions` table with scopes + auth_factors.
- Tests for happy path, expired nonce, reused nonce, revoked device.

Implementation plan (service)
1) Schema: ensure `sessions` table exists (INF-01) with fields from spec plus `challenge_nonce`, `challenge_expires_at`, and `used_at` to block replay. Add index on `(account_id, device_id, expires_at)`.

2) Challenge endpoint (`/auth/challenge`):
   - Handler in `service/src/identity/http/auth.rs` issues `{ challenge_id, nonce, expires_at }` and persists nonce tied to `(account_id, device_id)`.
   - Validate device is active: `devices.revoked_at IS NULL` and delegation not expired.
   - Use cryptographically secure random nonce (32 bytes) and short expiry (e.g., 5 minutes).

3) Verify endpoint (`/auth/verify`):
   - Accept `{ challenge_id, account_id, device_id, signature }` where signature covers canonical `{ challenge_id, nonce, account_id, device_id }`.
   - Fetch stored nonce, ensure not expired/used, and mark as used in a transaction.
   - Verify device delegation against current root (BE-02 + BE-04 state) and signature against device public key.
   - Issue session row with `scopes` (start with minimal `user:read`), `auth_factors` noting `cryptographic:true` and optionally others.
   - Return session token (JWT or random opaque string). Use `tower-http::trace` middleware for request logging.

4) Middleware: add `identity::auth::session_extractor` that validates the session token on incoming requests and exposes `{ account_id, device_id, scopes }` in request extensions for use by device routes.

5) Tests: create integration test that creates account, issues challenge, signs response with device key (use test keypair from BE-02 fixtures), verifies success, and attempts reuse of same nonce (should fail). Add revoked device test by marking `revoked_at` before verify.

Verification
- `cd service && cargo test identity_challenge_login`.
- `skaffold test -p ci` to ensure challenge table migrations apply.
- Manual: run server, POST to `/auth/challenge`, sign payload with test key, POST to `/auth/verify`, and check `sessions` table.

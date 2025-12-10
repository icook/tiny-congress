# BE-07 Endorsements create and revoke

Goal: accept device-signed endorsement envelopes, store them as signed events, and expose revocation flow. Keep raw envelopes as source of truth.

Deliverables
- `POST /endorsements` and `POST /endorsements/:id/revoke` handlers.
- Event append for `EndorsementCreated` and `EndorsementRevoked` plus reducer to `endorsements` table.
- Validation of subject/topic/magnitude/confidence ranges.

Implementation plan (service)
1) Schema: ensure `endorsements` table exists (INF-01) with fields in spec plus `envelope JSONB NOT NULL`, `revoked_at`, and uniqueness to prevent duplicate active endorsements per `(author_account_id, subject_type, subject_id, topic)` if desired. Add GIN index on tags.

2) Create handler (`service/src/identity/http/endorsements.rs`):
   - Authenticate via session extractor (BE-06) to get `{ account_id, device_id }`.
   - Validate request body fields: subject_type in enum, magnitude within [-1,1], confidence [0,1]. Canonicalize payload and verify device signature with BE-02.
   - Build sigchain event with next seqno and append using BE-01.
   - Reducer inserts `endorsements` row with envelope JSON and `created_at`.

3) Revoke handler: accept device-signed revocation envelope referencing endorsement_id or statement_hash.
   - Verify signature by the same author device (or other author device for account) and append `EndorsementRevoked` event.
   - Reducer sets `revoked_at` on `endorsements` row.

4) Exposure: add query handler `GET /users/:id/endorsements?topic=...` that reads from `endorsements` table (revoked filtered out) and returns raw envelopes plus derived aggregates (BE-08 will populate aggregates).

5) Tests: integration tests inserting endorsement via API, verifying DB row and aggregate placeholder. Negative tests: bad signature (400), revoked device (403), invalid range (422). Revoke test ensures revoked endorsement no longer returned.

Verification
- `cd service && cargo test identity_endorsements`.
- `skaffold test -p ci` after migrations.

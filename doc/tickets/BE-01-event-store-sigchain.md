# BE-01 Event store + sigchain

Goal: land the append-only signed event log that powers the per-account sigchain. Each event is stored canonically, chained by prev_hash, and validated on write.

Deliverables
- Migration adds `signed_events` table with uniqueness on `(account_id, seqno)` plus hash and envelope storage.
- Repository helper to append a signed event transactionally and enforce sigchain invariants.
- Data structures for canonical sigchain links (seqno, prev_hash, ctime, link_type, body).
- Tests that reject out-of-order seqnos, bad prev_hash, and tampered signatures.

Implementation plan (service)
1) Schema: add `service/migrations/02_identity_event_store.sql` that creates `signed_events`:
   - `event_id UUID PRIMARY KEY DEFAULT gen_random_uuid()`.
   - `account_id UUID NOT NULL` (FK to `accounts` once that table lands; keep FK deferrable or add in follow-up migration after accounts exist).
   - `seqno BIGINT NOT NULL` with `UNIQUE(account_id, seqno)` and partial index on `(account_id, seqno DESC)` for tail lookups.
   - `event_type TEXT NOT NULL` (matches payload_type e.g., DeviceDelegation, Endorsement, RootRotation).
   - `canonical_bytes_hash BYTEA NOT NULL` (sha256 of canonicalized envelope bytes).
   - `envelope_json JSONB NOT NULL` (entire envelope including signer and sig).
   - `created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()`.
   - Add check to prevent negative seqno. Add `prev_hash` inside payload; no separate column needed because we hash canonical bytes.

2) Domain structs: under `service/src/identity/sigchain/` add `mod.rs` + `link.rs` with `SigchainLink` (seqno, prev_hash, ctime, link_type, body) and `SignedEvent` (account_id, seqno, event_type, canonical_bytes, canonical_hash, envelope: serde_json::Value). Derive serde for JSON parsing.

3) Repo helper: add `service/src/identity/repo/event_store.rs` with `append_signed_event(pool: &PgPool, event: SignedEvent) -> Result<(), anyhow::Error>` that:
   - Fetches last `(seqno, canonical_bytes_hash)` for account_id to validate `seqno == last_seqno + 1` (or 1 for first) and `prev_hash` matches last canonical hash when seqno > 1.
   - Verifies signature and canonicalization via crypto helpers from BE-02 before writing.
   - Inserts row inside a transaction; error out on conflicts.
   - Expose helper `fetch_events(account_id)` for reducers to replay.

4) Wire-up: expose `pub mod identity` from `service/src/lib.rs`; include `sigchain` + `repo` modules. Keep router additions for later tickets.

5) Tests: add `service/tests/identity_event_store.rs` (or `service/src/identity/repo/tests.rs`) that spins up PgPool via existing `db::setup_database`, truncates `signed_events`, and covers:
   - Happy path append for seqno 1 and 2 with valid prev_hash.
   - Reject seqno gaps (e.g., appending seqno 3 first).
   - Reject prev_hash mismatch (mutate canonical bytes before append).
   - Reject tampered signature (flip envelope bytes) once BE-02 crypto is available.
   - Verify `canonical_bytes_hash` matches sha256(canonical_bytes).

Verification
- `cd service && cargo test identity_event_store` (or full `cargo test`).
- Run `skaffold test -p ci` to ensure migrations apply in containerized CI path once INF-01 adds new SQL.
- Manual smoke: `psql` into local DB and confirm `signed_events` rows grow monotonically per account.

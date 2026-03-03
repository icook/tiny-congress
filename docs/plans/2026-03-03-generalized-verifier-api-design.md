# Generalized Verifier API Design

**Ticket:** #381
**Date:** 2026-03-03
**Status:** Approved

## Problem

The current ID.me integration is a special-case path that bypasses the general verifier architecture. Verifier accounts exist as a separate entity type with no programmatic API. Adding new identity providers would require replicating special-case wiring. Automated services (demo seeder, admin tools) must use direct DB access.

## Design Decisions

### Verifiers are accounts

A verifier is a regular user account that holds a device key and has been endorsed with topic `"authorized_verifier"`. There is no separate verifier entity type or auth mechanism. One auth system (Ed25519 device key signatures), one account type.

This means:
- Verifiers authenticate via the existing `AuthenticatedDevice` flow
- Verifier privileges are granted and revoked through the endorsement system itself
- Full cryptographic audit trail — every endorsement is traceable to a specific device key
- Bot accounts, admin tools, and future third-party verifiers all follow the same pattern

### Genesis: config-driven bootstrap with nullable issuer

At startup, the server reads a list of verifier public keys from configuration:

```
TC_VERIFIERS='[
  {"name": "idme", "public_key": "<base64url-ed25519-pubkey>"},
  {"name": "demo-seeder", "public_key": "<base64url-ed25519-pubkey>"}
]'
```

For each entry, the server ensures:
1. An account exists with that root public key (create if missing)
2. An `"authorized_verifier"` endorsement exists for that account

Genesis endorsements have `issuer_id = NULL`, meaning "configured by the platform operator." This is the only place NULL issuer is allowed — it distinguishes platform-configured trust anchors from earned trust (where `issuer_id` points to a real verifier account).

Adding a new verifier is a config change, not a code change. Different environments (dev, staging, prod) can have different verifier sets.

### API key hashing: not applicable

Since verifiers authenticate via device key signatures (not API keys), there is no `api_key_hash` column. The original ticket's proposal for Bearer token auth is superseded by this design.

### Sybil checks: optional external identity

The `POST /verifiers/endorsements` endpoint accepts an optional `external_identity` field. If provided, the server performs dedup checks (same logic as current `link_identity_if_new`). If omitted, the server trusts the verifier to have performed its own checks.

## Schema Changes

### `reputation__endorsements` (modified)

```sql
-- issuer_id FK changes: reputation__verifier_accounts(id) → accounts(id), nullable
ALTER TABLE reputation__endorsements
  DROP CONSTRAINT ...,  -- old FK
  ALTER COLUMN issuer_id DROP NOT NULL,
  ADD CONSTRAINT fk_endorsements_issuer
    FOREIGN KEY (issuer_id) REFERENCES accounts(id);

-- Unique constraint widens: multiple verifiers can endorse same (subject, topic)
DROP INDEX uq_endorsements_subject_topic;
CREATE UNIQUE INDEX uq_endorsements_subject_topic_issuer
  ON reputation__endorsements (subject_id, topic, issuer_id);

-- Prevent duplicate genesis endorsements (NULL != NULL in PG UNIQUE)
CREATE UNIQUE INDEX uq_endorsements_genesis
  ON reputation__endorsements (subject_id, topic) WHERE issuer_id IS NULL;
```

### `reputation__verifier_accounts` (dropped)

Table is dropped after migrating existing data. Existing verifier account IDs are mapped to new account IDs during migration.

### `reputation__external_identities` (unchanged)

Sybil prevention table stays as-is.

### `has_endorsement` semantics

Unchanged — `WHERE subject_id = $1 AND topic = $2 AND revoked_at IS NULL` still returns true if *any* verifier has endorsed the subject for that topic.

## New Endpoint

```
POST /verifiers/endorsements
Authorization: <device-key-signature>  (existing AuthenticatedDevice)

{
  "username": "alice",
  "topic": "identity_verified",
  "evidence": { ... },                        // optional
  "external_identity": {                       // optional
    "provider": "idme",
    "provider_subject": "abc123"
  }
}
```

Processing:
1. `AuthenticatedDevice` validates device key signature (existing)
2. Check caller has `"authorized_verifier"` endorsement → 403 if not
3. Resolve `username` → `account_id` → 404 if not found
4. If `external_identity` provided, perform sybil dedup → 409 if already linked elsewhere
5. Create endorsement with `issuer_id = caller's account_id` → 409 if duplicate
6. Return created endorsement

Error mapping:

| Condition | Status |
|-----------|--------|
| Invalid/missing device key signature | 401 |
| Caller lacks `authorized_verifier` endorsement | 403 |
| Username not found | 404 |
| Endorsement already exists (same subject+topic+issuer) | 409 |
| External identity linked to different account | 409 |

## ID.me Adapter: Separate Service

The ID.me OAuth handler is extracted from the TC server into a standalone binary: `tc-idme-verifier`.

### Architecture

The verifier service:
- Holds its own Ed25519 device key pair
- Is registered as a TC account with `"authorized_verifier"` endorsement (via config bootstrap)
- Serves OAuth endpoints for the ID.me flow
- Calls TC API via the generated Rust client to create endorsements

### Authentication flow

```
Frontend → idme-verifier: GET /authorize (signed with user's device key)
idme-verifier → TC API: GET /me (forwarded auth) → validates user, returns account info
idme-verifier → ID.me: OAuth redirect
ID.me → idme-verifier: callback with code
idme-verifier → ID.me: exchange code, fetch userinfo
idme-verifier → TC API: POST /verifiers/endorsements (signed with verifier's device key)
idme-verifier → Frontend: redirect with success/error
```

Users authenticate with the verifier service using their TC device key. The verifier validates by forwarding to `GET /me` on the TC API. This eliminates username spoofing — the user proves identity cryptographically.

### What moves out of TC server

- `service/src/reputation/http/idme.rs` — entire file removed
- ID.me config (`IdMeConfig`) — moves to the verifier service
- HMAC state signing — moves to the verifier service (its own secret)
- `bootstrap_idme_verifier` — replaced by config-driven genesis

### What stays in TC server

- `POST /verifiers/endorsements` endpoint (new, general)
- `reputation__external_identities` table and sybil dedup logic (called by the endpoint when `external_identity` is provided)
- `GET /me` endpoint (existing, used by verifier to validate users)

## Generated Rust API Client

A new crate `tc-api-client` generated via [Progenitor](https://github.com/oxidecomputer/progenitor) from `web/openapi.json` (the existing OpenAPI spec).

### Codegen pipeline

```
utoipa annotations (Rust handlers)
  → export_openapi binary
  → web/openapi.json
  → openapi-typescript (frontend TS client, existing)
  → progenitor (Rust client, new)
  → service/crates/tc-api-client/
```

### Client crate responsibilities

- Typed request/response structs (generated from spec)
- Device key request signing (manual layer using `tc-crypto`)
- Client struct configured with base URL + signing key

### Consumers

- `tc-idme-verifier` — validate user device keys, create endorsements
- Demo seeder — create endorsements via API instead of direct DB access
- Future verifier services, bots, admin tools

## Demo Seeder Migration

The demo seeder (`service/src/seed/accounts.rs` or equivalent) is updated to use `tc-api-client` instead of calling repo functions directly. It authenticates as a verifier account (bootstrapped via config) and calls `POST /verifiers/endorsements`.

## Out of Scope

- New identity providers (just the general framework)
- Verifier management UI
- Revoking endorsements via API
- Topic-level authorization for verifiers (verifiers are unrestricted)
- Trust graph weighting of endorsements

# Trust UI Gap Analysis

**Date:** 2026-03-11
**Source:** Design state document + TRD cross-referenced against codebase at `9c26720` (PR #555)

## Summary

PR #555 shipped the full backend trust engine (~6,000 lines). The frontend is unchanged — it still presents the pre-trust demo (signup → verify → vote). This document captures the concrete delta between design intent and current implementation, organized by user-facing capability.

---

## What Users See Today

1. **Home** → marketing page
2. **Sign Up** → username + backup password → key generation → success screen with "Browse Rooms"
3. **Login** → username + password → Argon2id KDF → device key recovery
4. **Rooms** → list of rooms (public, no auth)
5. **Poll** → slider voting with eligibility gate:
   - Not signed in → "Sign up or log in" alert
   - Signed in but unverified → "Verify your identity" alert with link to external verifier
   - Verified → sliders enabled, submit button
6. **Settings** → device list, current device badge
7. **Navbar** → Home, Rooms, About (guest) | + Settings, Verified/Unverified badge (auth)
8. **Verify Callback** → handles redirect from external verifier, redirects to /rooms

## What the Design Documents Expect

The TRD and design state document describe a trust-centric experience where:

### Trust Score Visibility
- Users see their **trust distance** (weighted hops from seed) and **path diversity** (distinct endorsers)
- A **tier badge** (Community / Congress) shows what rooms they qualify for
- An **influence budget** shows remaining endorsement capacity (starts at 10.0)
- These are the "aha moment" — users understand their position in the web of trust

### Endorsement Flow
- Users can **endorse** other users (costs influence budget, queued async)
- Users can **revoke** endorsements (returns staked influence)
- Users can **denounce** bad actors (costs influence, max 2 active denouncements)
- Daily action quota: 5 actions/day

### Invite / Handshake Flow
- **QR code generation**: Create a short-lived invite, encode as QR
- **QR scanning**: Recipient scans, accepts invite, trust edge created
- **Link sharing**: Alternative to QR — send invite URL
- Physical co-presence ("the ritual") creates high-weight trust edges
- The TRD describes JWT-signed QR payloads, but the backend uses invite IDs with expiry + single-use constraints (simpler, equivalent security)

### Room Constraint System
- Rooms have a `constraint_type` + `constraint_config` (JSONB)
- Three constraint presets:
  - **EndorsedBy** — must be reachable from anchor in trust graph
  - **Community** — trust distance ≤ threshold AND path diversity ≥ threshold
  - **Congress** — stricter sybil resistance via path diversity only
- The design envisions rooms displaying their constraint requirements so users understand what they need to qualify

### Trust Tree Visualization
- The design describes a graph visualization showing the user's trust connections
- This is positioned as a key differentiator and engagement driver
- No implementation exists on either side

---

## Gap Table

| Capability | Backend | Frontend | Gap |
|---|---|---|---|
| Trust score computation | CTE + worker, cached snapshots | None | Full build needed |
| Trust score display | `GET /trust/scores/me` | None | API client + component |
| Influence budget | `GET /trust/budget` | None | API client + component |
| Tier badges (Community/Congress) | Constraint system computes eligibility | Navbar shows binary Verified/Unverified | Replace badge logic |
| Endorsement creation | `POST /trust/endorse` (202) | None | API client + UI (form? inline?) |
| Endorsement revocation | `POST /trust/revoke` (202) | None | API client + UI |
| Denouncement | `POST /trust/denounce` (202) | None | API client + UI — design question: how prominent? |
| Invite creation | `POST /trust/invites` | None | API client + QR generation |
| Invite listing | `GET /trust/invites/mine` | None | API client + table |
| Invite acceptance | `POST /trust/invites/{id}/accept` | None | API client + accept page |
| QR handshake (generate) | Invite endpoint exists | None | QR library + component |
| QR handshake (scan) | Accept endpoint exists | None | Camera API + scanning library |
| Room constraint display | `constraint_type` + `constraint_config` in room record | `Room` type only has `eligibility_topic` | Type mismatch — may or may not serialize |
| Poll eligibility messaging | Backend returns specific constraint failure reason | Frontend shows generic "verify your identity" | Surface backend error detail |
| Trust tree visualization | Score snapshots exist, graph data in endorsements table | None | Major design + build effort |
| Demo data seeding | Tables exist | None | Seed script needed for demo |

## Specific Code-Level Findings

### Room Type Mismatch
- Backend `RoomRecord` has `constraint_type: String` and `constraint_config: serde_json::Value` (added in migration 14)
- Frontend `Room` at `web/src/features/rooms/api/client.ts:12-19` only has `eligibility_topic: string`
- **Status:** Needs investigation — check whether the backend serializes these new fields in GET /rooms responses. If yes, frontend silently ignores them. If no, they're backend-only.

### Dead Code
- `HasEndorsementResponse` and `checkEndorsement()` at `web/src/features/rooms/api/client.ts:89-129` appear unused — the frontend doesn't call `checkEndorsement` anywhere. Likely from an earlier endorsement check approach that was replaced by the verification status hook. Needs grep confirmation.

### Verification vs Trust
- The frontend uses a **binary** verification model: `topic === 'identity_verified' && !revoked`
- The backend now uses a **graduated** trust model: distance + diversity + constraints
- These are layered, not contradictory — verification is the entry point, trust score is the deeper signal
- But the UI only shows the binary layer. The design docs expect both.

### Error Path
- Vote rejection returns from `rooms/service.rs` via `VoteError::NotEligible` with a human-readable `reason` string
- This flows through `fetchClient.ts` error handling → `voteMutation.error.message`
- The error message should now contain constraint-specific language (e.g., "insufficient trust distance") rather than generic "not verified"
- **Needs verification** — trace the exact error format from constraint check through HTTP response

### Backend Trust Endpoints Not in OpenAPI
- The trust HTTP handlers in `service/src/trust/http/mod.rs` may not be annotated with utoipa's `#[utoipa::path]` macros
- If so, `web/openapi.json` doesn't include them and `just codegen` won't generate TypeScript types
- The trust API client will need hand-written types matching the Rust response structs

---

## Design Questions (Open — Feed Into Brainstorming)

These are questions the gap analysis surfaced that need design decisions, not just implementation:

1. **Where does trust score live in the UI?** Settings page? Dedicated /trust page? Navbar? All three?
2. **How do users discover endorsement?** Is it a button on another user's profile? A standalone action? Part of the handshake flow only?
3. **How prominent is denouncement?** The TRD includes it, but for a friends-and-family demo, is it confusing/premature?
4. **What does the eligibility gate look like for trust-gated rooms?** Today it says "verify your identity." For Community/Congress rooms, should it say "you need endorsements from 2+ trusted members" with a progress indicator?
5. **Is the trust tree visualization in scope for March 20?** It's the design doc's "aha moment" but it's the largest single piece of frontend work.
6. **Mobile-first or desktop-first?** The QR handshake is inherently mobile. Is the trust dashboard also mobile-primary?

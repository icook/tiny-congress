# Trust UI Gap Analysis

**Date:** 2026-03-11
**Updated:** 2026-03-12 (aligned with ADR series 017-021, PR #630)
**Source:** Design state document + TRD cross-referenced against codebase at `9c26720` (PR #555)

## Summary

PR #555 shipped the backend trust engine (~6,000 lines). The frontend is unchanged — it still presents the pre-trust demo (signup → verify → vote). This document captures the delta between design intent and current implementation.

> **Architecture update (2026-03-12):** The backend implements a continuous influence
> model with real-time processing. ADRs 020 and 021 (Proposed) call for discrete
> endorsement slots and 24h batch reconciliation respectively. **The backend itself
> needs changes before the frontend can fully align.** Sections marked **[UPDATED]**
> reflect the target architecture per ADRs, not current backend state.

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

### Trust Score Visibility [UPDATED]
- Users see their **trust distance** (weighted hops from seed) and **path diversity** (distinct endorsers) — from latest daily snapshot (ADR-021)
- Room eligibility is **room-configurable** (ADR-017) — rooms set their own distance/diversity thresholds, not platform-wide tiers
- An **endorsement slot count** shows remaining capacity: "2 of 3 endorsements used" (ADR-020, replaces influence budget)
- These are the "aha moment" — users understand their position in the web of trust

### Endorsement Flow [UPDATED]
- Users **endorse** via handshakes (ADR-018): Physical QR (1.0), Remote (0.7), Social Referral (0.3). Endorsement = handshake, not a separate action.
- Each endorsement occupies one **slot** (ADR-020, k=3 demo). To endorse a new person when full, must revoke an existing one.
- Users can **revoke** endorsements (frees the slot)
- Users can **denounce** bad actors (d=2 permanent budget, no graph effect yet — ADR-020)
- **Daily action budget** (ADR-020/021): TBD (1-3 for demo), renewable, use-it-or-lose-it
- **Actions are declared intentions** (ADR-021): applied at next daily batch, retractable before then

### Invite / Handshake Flow
- **QR code generation**: Create a short-lived invite, encode as QR
- **QR scanning**: Recipient scans, accepts invite, trust edge created
- **Link sharing**: Alternative to QR — send invite URL
- Physical co-presence ("the ritual") creates high-weight trust edges
- The TRD describes JWT-signed QR payloads, but the backend uses invite IDs with expiry + single-use constraints (simpler, equivalent security)

### Room Constraint System [UPDATED]
- Rooms have a `constraint_type` + `constraint_config` (JSONB)
- Three constraint presets exist as reference implementations:
  - **EndorsedBy** — must be reachable from anchor in trust graph
  - **Community** — trust distance ≤ threshold AND path diversity ≥ threshold
  - **Congress** — stricter sybil resistance via path diversity only
- **Thresholds are room-configurable, not platform constants (ADR-017).** Rooms are independent relying parties that consume trust graph signals and define their own gating policies.
- The design envisions rooms displaying their specific constraint requirements so users understand what they need to qualify

### Trust Tree Visualization
- The design describes a graph visualization showing the user's trust connections
- This is positioned as a key differentiator and engagement driver
- No implementation exists on either side

---

## Gap Table [UPDATED]

| Capability | Backend State | Backend Target (ADRs) | Frontend | Gap |
|---|---|---|---|---|
| Trust score computation | CTE + worker, cached snapshots | Same (ADR-019) | None | Frontend build needed |
| Trust score display | `GET /trust/scores/me` | Returns daily snapshot (ADR-021) | None | API client + component |
| Endorsement slots | `GET /trust/budget` returns float influence | Slot count: k=3 demo (ADR-020) | None | **Backend change** + frontend build |
| Room eligibility | Constraint system with hardcoded presets | Room-configurable thresholds (ADR-017) | Binary Verified/Unverified | Replace badge logic, surface room policy |
| Endorsement creation | `POST /trust/endorse` (202, immediate queue) | Declared intention, batch-applied (ADR-021) | None | **Backend change** + UI with pending state |
| Endorsement revocation | `POST /trust/revoke` (202) | Retractable before batch (ADR-021) | None | **Backend change** + UI |
| Denouncement | `POST /trust/denounce` (202, costs influence) | d=2 permanent budget, no graph effect (ADR-020) | None | **Backend change** + UI |
| Invite creation | `POST /trust/invites` | Same | None | API client + QR generation |
| Invite listing | `GET /trust/invites/mine` | Same | None | API client + table |
| Invite acceptance | `POST /trust/invites/{id}/accept` | Creates edge, weight=0.3 (ADR-018) | None | API client + accept page |
| QR handshake (generate) | Invite endpoint exists | JWT-signed QR, weight=1.0 (ADR-018) | None | QR library + component |
| QR handshake (scan) | Accept endpoint exists | Same | None | Camera API + scanning library |
| Room constraint display | `constraint_type` + `constraint_config` | Room-configurable (ADR-017) | Only `eligibility_topic` | Type mismatch — needs investigation |
| Poll eligibility messaging | Returns `VoteError::NotEligible` | Two-layer message (ADR-017) | Generic "verify your identity" | Surface constraint-specific + room-specific detail |
| Pending actions UX | Action queue exists | Retractable declarations (ADR-021) | None | **New concept** — show pending vs applied |
| Batch reconciliation | Real-time worker | Daily batch at fixed time (ADR-021) | None | **Backend change** + "scores update tomorrow" messaging |
| Trust tree visualization | Score snapshots + endorsement graph | Same | None | Major design + build effort |
| Demo data seeding | Tables exist | Same | None | Seed script needed |

## Specific Code-Level Findings

### Room Type Mismatch
- Backend `RoomRecord` has `constraint_type: String` and `constraint_config: serde_json::Value` (added in migration 14)
- Frontend `Room` at `web/src/features/rooms/api/client.ts:12-19` only has `eligibility_topic: string`
- **Status:** Needs investigation — check whether the backend serializes these new fields in GET /rooms responses. If yes, frontend silently ignores them. If no, they're backend-only.

### Dead Code
- `HasEndorsementResponse` and `checkEndorsement()` at `web/src/features/rooms/api/client.ts:89-129` appear unused — the frontend doesn't call `checkEndorsement` anywhere. Likely from an earlier endorsement check approach that was replaced by the verification status hook. Needs grep confirmation.

### Verification vs Trust [UPDATED — two-layer architecture, ADR-017]
- The frontend uses a **binary** verification model: `topic === 'identity_verified' && !revoked`
- The backend now uses a **graduated** trust model: distance + diversity + constraints
- **ADR-017 formalizes this as a two-layer split:** Platform trust (humanity/Sybil resistance) is the identity layer — "are you a verified human?" Rooms are the permission layer — "does this room want to hear from you?"
- The UI needs to surface both layers: "Your platform trust position" (distance, diversity, slots) and "This room requires..." (room-specific thresholds)
- Verification (binary) maps to the *minimum* platform trust signal — you've been attested as human by at least one path. Trust score is the deeper graduated signal.

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
7. **[NEW] How to present pending vs applied state?** Under batch reconciliation (ADR-021), the UI must show both "what you've declared today" and "your current trust position from last batch." What's the right UX pattern — a pending/applied toggle? Inline annotations? A timeline?
8. **[NEW] What does "2 of 3 endorsements used" look like?** The slot scarcity display (ADR-020) is the central UX element. Is it a progress bar? Circles? A card?

---

## Backend Changes Required Before Frontend Can Align

These backend items block full M3 implementation. The frontend can start building against current endpoints, but the final UX depends on these changes landing.

| Backend change | ADR | Blocking what |
|---|---|---|
| Replace continuous influence with discrete slot count | ADR-020 | Slot display ("2 of 3"), budget endpoint shape |
| Implement batch reconciliation (queue → daily batch) | ADR-021 | Pending vs applied state UX |
| Add retractable action declarations | ADR-021 | Undo/retract UX for pending endorsements |
| Denouncement as permanent budget (d=2), not influence cost | ADR-020 | Denouncement UI flow |
| Verifier/platform account slot exemption | ADR-020 + 008 audit | Demo bootstrapping |
| Trust anchor bootstrap (distance=0 for seed node) | ADR-019 audit | Any trust score display for first user |

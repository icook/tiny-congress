# M3 Trust UI — Brainstorming Brief

**Date:** 2026-03-11
**Purpose:** Feed into a brainstorming session before planning M3 implementation. M3 is a design problem, not just an implementation task.

---

## What This Brief Is For

The backend trust engine shipped (PR #555). The frontend needs to surface it. But "add components that call the endpoints" is the wrong frame — the trust UI is the primary way users understand and engage with the governance model. Getting the UX right matters more than getting it built fast.

This brief captures what's known, what's uncertain, and what questions a brainstorming session should resolve.

## Constraints

- **Audience:** Non-technical friends and family on their phones. March 20 demo.
- **Bar:** "Can a non-technical person open this link on their phone, sign up, vote, and see results without anyone explaining it to them?"
- **Backend is done:** All trust endpoints exist and return 202 for mutations. Scores are cached snapshots. No backend work needed for M3.
- **Design system:** Mantine v7. ADR-005 mandates Mantine-first styling. No custom CSS unless Mantine can't do it.
- **Auth pattern:** All trust endpoints require signed requests via `signedFetchJson()` with device key.

## What Exists

### Backend Endpoints (Ready to Consume)
```
GET  /trust/scores/me    → ScoreSnapshot[] (distance, diversity, centrality, computed_at)
GET  /trust/budget       → BudgetResponse (total, staked, spent, available influence)
POST /trust/endorse      → 202 (queued — weight, subject_id, attestation)
POST /trust/revoke       → 202 (queued — subject_id)
POST /trust/denounce     → 202 (queued — target_id, reason, influence_cost)
POST /trust/invites      → {id, expires_at} (envelope, delivery_method, attestation)
GET  /trust/invites/mine → Invite[] (id, delivery_method, accepted_by, expires_at, accepted_at)
POST /trust/invites/{id}/accept → {endorser_id, accepted_at}
```

### Frontend Patterns to Follow
- `web/src/features/verification/` — API client + query hook + URL builder. Clean, minimal.
- `web/src/features/rooms/api/` — Client types + TanStack Query hooks. Good separation.
- `web/src/pages/Poll.page.tsx` — Complex page with multiple query hooks, conditional rendering by auth/verification state.

### Current UX Flow
```
Home → Sign Up → [external verifier] → Verify Callback → Rooms → Poll → Vote
                                                              ↑
Home → Login ─────────────────────────────────────────────────┘
```

### Trust UX Flow (Design Intent)
```
Home → Sign Up → [external verifier] → Verify Callback → Trust Dashboard ← Rooms → Poll → Vote
                                              ↑                    ↓                    ↑
                                         QR Handshake ←── Generate QR             Trust gate
                                              ↓                                   (not just verify gate)
                                         Accept Invite → Score updates → Room unlocks
```

## Open Design Questions

### 1. Information Architecture
- **Where does trust live?** Dedicated `/trust` page? Section on Settings? Floating widget?
- **Is trust a "feature" or a "status"?** Features get pages. Statuses get badges/indicators.
- **Navigation:** Does "Trust" appear in the main nav? Only when authenticated? Only after verification?

### 2. Trust Score Presentation
- **What metaphor?** Raw numbers (distance: 2.3, diversity: 4)? Progress bars? A thermometer? A network diagram?
- **Tier framing:** "Community Member" vs "Congress Member" — are these meaningful to a non-technical person? Should they be renamed for the demo?
- **Empty state:** A freshly verified user with no trust score. This is the most common state at demo time. What do they see? "Get endorsed" is the CTA — but how?

### 3. Endorsement UX
- **Discovery:** How does User A endorse User B? There's no user profile page. No user search. No contact list.
- **Is endorsement separate from the QR handshake?** The TRD positions them together (physical co-presence → endorsement). Should there be a way to endorse without a QR code?
- **Weight:** Endorsements have a `weight` field (0.0–1.0). Is this user-facing? Or always 1.0 for the demo?
- **Feedback loop:** Endorsements are async (202). How does the user know it worked? Poll? Optimistic update? Push notification?

### 4. Invite/Handshake UX
- **QR code size:** Mobile screens are small. QR codes need to be scannable from another phone's camera. Minimum viable size?
- **Failure states:** Camera permission denied. QR code expired. Already accepted. Network error mid-scan. Each needs a clear message.
- **The ritual:** Two people standing together. One generates, one scans. Should there be a "waiting for scan" state on the generator's screen? A confirmation on both sides?

### 5. Eligibility Messaging
- **Today:** "You need to verify your identity to vote in this room." + Verify Now button.
- **Tomorrow:** Rooms have different constraint types. The message needs to explain what's needed:
  - EndorsedBy: "You need to be endorsed by a trusted member"
  - Community: "You need trust distance ≤ 6 and path diversity ≥ 1"
  - Congress: "You need path diversity ≥ 2 from at least 2 independent trust paths"
- **Challenge:** These are graph-theoretic concepts. How do you explain "path diversity" to a non-technical person?
- **Progress indicator?** "You're 1 endorsement away from qualifying" would be powerful but requires computing hypothetical scores.

### 6. Scope for March 20
- What's the minimum trust UI that makes the demo coherent?
- Is the trust tree visualization in scope? It's the "aha moment" per the design doc, but it's the largest piece.
- Can we get by with: trust score badge + QR handshake + eligibility messages? (Skip: endorsement management, denouncement, invite table, visualization)

## Inspirations / References

- **Keybase:** Showed trust chains simply — "Alice verified Bob who verified you" as a breadcrumb.
- **Signal safety numbers:** QR code scanning for verification, with clear success/failure states.
- **Web of Trust (PGP):** The conceptual ancestor. Failed partly because the UX was incomprehensible.
- **LinkedIn connections:** Degree-of-separation display ("2nd connection") is the closest mainstream parallel to trust distance.

## Output Expected

A brainstorming session should produce:
1. **Information architecture decision** — where trust lives in the nav/page structure
2. **Component inventory** — what components are needed, rough wireframe-level layout
3. **Scope cut** — what's in for March 20, what's deferred
4. **Metaphor/framing** — how trust concepts are presented to non-technical users
5. **Empty state design** — what a fresh user sees (this IS the demo first impression)

# M3 Trust UI — Brainstorming Brief

**Date:** 2026-03-11
**Updated:** 2026-03-12 (aligned with ADR series 017-021, PR #630)
**Purpose:** Feed into a brainstorming session before planning M3 implementation. M3 is a design problem, not just an implementation task.

---

> **Architecture update (2026-03-12):** ADRs 017-021 formalize trust architecture
> decisions that change several assumptions in this brief. Sections marked with
> **[UPDATED]** have been revised. Key shifts:
>
> - **Two-layer split (ADR-017):** Platform trust (Sybil resistance) vs communication permission (room-level gating). Rooms are independent relying parties.
> - **Discrete endorsement slots (ADR-020):** Replaces continuous influence budget. Users have k=3 slots (demo). "2 of 3 endorsements used."
> - **24h batch reconciliation (ADR-021):** Actions are declared intentions, reconciled at EOD. UI must show pending vs applied state.
> - **Handshake weights are context-determined (ADR-018):** Physical QR=1.0, Remote=0.7, Social Referral=0.3. Not user-chosen.
> - **Denouncements don't affect graph yet (ADR-020):** d=2 permanent budget. Mechanism TBD.
>
> See also: `.plan/2026-03-12-sponsorship-risk-design.md` on `test/624-trust-graph-simulation` for full design state.

---

## What This Brief Is For

The backend trust engine shipped (PR #555). The frontend needs to surface it. But "add components that call the endpoints" is the wrong frame — the trust UI is the primary way users understand and engage with the governance model. Getting the UX right matters more than getting it built fast.

This brief captures what's known, what's uncertain, and what questions a brainstorming session should resolve.

## Constraints

- **Audience:** Non-technical friends and family on their phones. March 20 demo.
- **Bar:** "Can a non-technical person open this link on their phone, sign up, vote, and see results without anyone explaining it to them?"
- **Backend needs updates:** Trust endpoints exist but implement continuous influence model. ADR-020 (slots) and ADR-021 (batch) are Proposed and require backend changes before the frontend can fully align. Endpoint signatures may change.
- **Design system:** Mantine v7. ADR-005 mandates Mantine-first styling. No custom CSS unless Mantine can't do it.
- **Auth pattern:** All trust endpoints require signed requests via `signedFetchJson()` with device key.

## What Exists

### Backend Endpoints [UPDATED]
```
GET  /trust/scores/me    → ScoreSnapshot[] (distance, diversity, centrality, computed_at)
                           ↳ Returns latest daily snapshot (ADR-021), not real-time
GET  /trust/budget       → BudgetResponse (total, staked, spent, available influence)
                           ↳ WILL CHANGE: ADR-020 replaces with slot count (k=3 demo)
                             New shape TBD: {slots_total, slots_used, slots_available}
POST /trust/endorse      → 202 (queued — weight, subject_id, attestation)
                           ↳ Weight determined by handshake context (ADR-018), not user input
                           ↳ Under ADR-021: declared intention, applied at next batch
POST /trust/revoke       → 202 (queued — subject_id)
                           ↳ Under ADR-021: declared intention, retractable before batch
POST /trust/denounce     → 202 (queued — target_id, reason, influence_cost)
                           ↳ WILL CHANGE: ADR-020 uses permanent d=2 budget, not influence cost
                           ↳ Currently has no graph effect (ADR-020)
POST /trust/invites      → {id, expires_at} (envelope, delivery_method, attestation)
GET  /trust/invites/mine → Invite[] (id, delivery_method, accepted_by, expires_at, accepted_at)
POST /trust/invites/{id}/accept → {endorser_id, accepted_at}
                           ↳ Creates edge with weight=0.3 (social referral, ADR-018)
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

### Trust UX Flow [UPDATED for batch model]
```
Home → Sign Up → Social Referral Invite → Trust Dashboard ← Rooms → Poll → Vote
                         ↑                      ↓                       ↑
                    QR Handshake ←── Generate QR                   Trust gate
                         ↓                                     (room-configurable,
                    Accept Invite                               ADR-017)
                         ↓
                    Action declared → Pending state shown → Next batch → Score updates → Room unlocks
```
**Key UX implication (ADR-021):** There is a delay between action and effect. The UI must clearly communicate: "Your endorsement has been declared. Trust scores update daily." This is a feature, not a bug — it forces intentionality.

## Open Design Questions

### 1. Information Architecture
- **Where does trust live?** Dedicated `/trust` page? Section on Settings? Floating widget?
- **Is trust a "feature" or a "status"?** Features get pages. Statuses get badges/indicators.
- **Navigation:** Does "Trust" appear in the main nav? Only when authenticated? Only after verification?

### 2. Trust Score Presentation
- **What metaphor?** Raw numbers (distance: 2.3, diversity: 4)? Progress bars? A thermometer? A network diagram?
- **Tier framing:** "Community Member" vs "Congress Member" — are these meaningful to a non-technical person? Should they be renamed for the demo?
- **Empty state:** A freshly verified user with no trust score. This is the most common state at demo time. What do they see? "Get endorsed" is the CTA — but how?

### 3. Endorsement UX [UPDATED]
- **Discovery:** How does User A endorse User B? There's no user profile page. No user search. No contact list.
- **Endorsement = handshake (ADR-018):** There are three handshake contexts: Physical QR (weight 1.0), Synchronous Remote (weight 0.7), Social Referral (weight 0.3). The handshake IS the endorsement — they're the same action. No separate "endorse" button.
- **Weight is not user-facing (ADR-018):** Determined by handshake context, not user choice. The UI shows the handshake type, not a weight number.
- **Slot scarcity is the UX hook (ADR-020):** "2 of 3 endorsements used" — this is the central design element. Users must choose who to endorse with their limited slots.
- **Feedback loop under batch (ADR-021):** Endorsements are declared intentions. The UI must show: (1) pending actions declared today, (2) last applied snapshot. "Your endorsement of Alice is pending. Scores update tomorrow."
- **Retraction:** Users can retract a declared action before the batch runs. The UI needs a way to undo a pending endorsement.

### 4. Invite/Handshake UX
- **QR code size:** Mobile screens are small. QR codes need to be scannable from another phone's camera. Minimum viable size?
- **Failure states:** Camera permission denied. QR code expired. Already accepted. Network error mid-scan. Each needs a clear message.
- **The ritual:** Two people standing together. One generates, one scans. Should there be a "waiting for scan" state on the generator's screen? A confirmation on both sides?

### 5. Eligibility Messaging [UPDATED]
- **Today:** "You need to verify your identity to vote in this room." + Verify Now button.
- **Two-layer framing (ADR-017):** Platform trust ("you are a verified human") vs room permission ("this room requires X"). The UI should separate these: "Your trust position" (platform) vs "This room requires" (room-specific).
- **Room thresholds are room-configurable (ADR-017):** Not platform constants. Each room sets its own distance/diversity requirements. The UI should display the room's specific policy, not hardcoded tier names.
- **Challenge:** Graph concepts need human language. "You're endorsed by 2 trusted people" is better than "path diversity ≥ 2."
- **Progress indicator?** "You're 1 endorsement away from qualifying" would be powerful but requires computing hypothetical scores. Under batch model, this is "after tomorrow's update, you may qualify."
- **Batch timing (ADR-021):** "You endorsed Alice today. Your trust score will update tomorrow. Once updated, you may qualify for this room."

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

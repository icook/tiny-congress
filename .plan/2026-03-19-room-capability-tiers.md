# Room Capability Tiers & Progressive Authorization

**Date:** 2026-03-19
**Status:** Design brief — not yet scoped into tickets
**Context:** Rooms currently have binary eligibility (endorsed_by / community / congress constraints). The vision is progressive authorization: users unlock capabilities as they meet escalating gates. This replaces binary in/out with a visible, room-configured capability graph.

---

## Core Concept

A room defines a set of **capability tiers**. Each tier:
- Has a **name** (Observer, Participant, Contributor, Steward, Owner)
- Unlocks a set of **capabilities** (vote, propose, configure, assign roles...)
- Has a **gate** — a constraint expression the user must satisfy to hold the tier

Tiers are NOT strictly ordered. They form a DAG — some capabilities may exist on parallel tracks (e.g., "Moderator" and "Contributor" aren't necessarily one above the other). The UX challenge of presenting a DAG vs a ladder is acknowledged and deferred.

### Room Owner tier

Every room has a required **Owner** tier, auto-assigned to the room creator. Owner can:
- Define and modify tier configurations
- Assign any tier to any account (up to their own level)
- Eventually: gate config changes behind elections (not at launch)

Owner is the root of authority within the room. It's analogous to the trust anchor at the platform level — a bootstrapping necessity that can be decentralized later.

---

## Example Configuration

```
Room: "Brand Ethics"
─────────────────────────────────────────────────────────
Tier: Observer            [any account]
  → read polls, see results, see evidence

Tier: Participant         [identity_verified]
  → vote on polls, submit evidence

Tier: Contributor         [endorsed_by(room_owner) OR endorsed_by_count(participants, 2)]
  → propose new poll topics, flag evidence

Tier: Moderator           [assigned_by(owner)]
  → manage poll lifecycle, remove flagged content

Tier: Owner               [auto: room creator]
  → configure room, manage tiers, assign roles
─────────────────────────────────────────────────────────
```

---

## Gate Types (constraint expressions)

Gates compose from existing primitives plus new room-level ones:

| Gate | Source | Status |
|------|--------|--------|
| `identity_verified` | Platform identity layer | Built |
| `endorsed_by(account)` | Trust graph — edge exists | Built (needs out-of-slot query) |
| `endorsed_by_count(tier, N)` | Trust graph — N edges from accounts holding a specific tier | New |
| `trust_distance(anchor, max)` | Trust engine | Built |
| `trust_diversity(anchor, min)` | Trust engine | Built |
| `assigned_by(tier)` | Room role assignment | New |
| `holds_tier(tier_name)` | Room tier resolution | New (recursive — a gate can require another tier) |

Room-level activity gates (e.g., "voted N times") are **aspirational** — not needed for initial implementation but the gate system should be extensible enough to add them later.

---

## The Endorsement Split

**Key architectural decision:** Endorsements are separated into slot-contributing and attestation-only.

```
Endorsement (relationship exists)     ← unlimited (or platform-trust-gated)
    │
    ├── In-slot: contributes to trust graph weight
    │   (capped at k=10, affects eligibility computation)
    │
    └── Out-of-slot: attestation only
        (rooms can query "does edge exist?" for access control)
```

### How this works

- Users can endorse **beyond their slot limit**. Out-of-slot endorsements are stored but do not participate in trust score computation.
- The endorser **explicitly chooses** which endorsements occupy slots. UX for this is TBD.
- Rooms can gate on endorsement *existence* (`endorsed_by(alice)`) regardless of slot status.
- The trust engine continues to use only in-slot endorsements for distance/diversity computation.

### Why this matters

1. **Endorsing becomes socially cheap.** Currently, endorsing someone costs a slot — it's a resource allocation decision. With unlimited out-of-slot endorsements, "I know this person" is free. "I trust this person with my graph weight" still costs a slot.
2. **Invites are just endorsements.** A room configured with `endorsed_by(owner)` as a gate gets invite functionality for free. The owner endorses the invitee (out-of-slot if they don't want to spend a slot), and the room sees the edge and grants the tier.
3. **No separate invite system needed.** The endorsement IS the invite. The room's tier config determines what endorsements qualify.

### Relationship to Q30 (Verifiers as graph participants)

This directly addresses the verifier slot budget problem from Q30. Verifiers can issue unlimited out-of-slot endorsements (attestations of identity verification). Rooms that require identity verification can gate on `endorsed_by(verifier_entity)` using out-of-slot edges. Only if the verifier's endorsement should carry trust graph weight does it need a slot.

Option B from Q30 ("light endorsement tier") is essentially what's described here, but framed as a property of endorsement storage rather than a separate tier. The implementation is the same: endorsements exist in the DB; the trust engine filters to in-slot only; rooms can query all.

---

## Appointed → Elected Progression

The tier system supports both appointment and election as role-assignment mechanisms:

**Phase 1 (now):** Owner appoints accounts to tiers via direct assignment.

**Phase 2 (future):** A room can run an **election poll** where the output is a role assignment. The polling infrastructure already exists — an election is just a poll whose `reduce()` output is a tier assignment rather than an opinion distribution. This means:
- Elections are a room module feature, not a separate system
- The same trust/eligibility gates apply to who can vote in an election
- Election results are transparent and auditable (same as poll results)

This is not needed at launch but the tier system should not preclude it.

---

## UX: Capability Transparency

The core UX principle: **the user always knows where they stand.**

```
┌─────────────────────────────────────────┐
│  Your role: Participant                 │
│  ✓ Identity verified                    │
│  ✓ Can vote and submit evidence         │
│                                         │
│  Next: Contributor                      │
│  ○ Need endorsement from room owner     │
│    or 2 endorsements from participants  │
└─────────────────────────────────────────┘
```

Every tier's gate is visible. The user sees:
- Which tiers they currently hold
- What capabilities those tiers unlock
- What's required for tiers they don't hold
- Their progress toward those requirements

This is philosophically aligned with TC's transparency stance. The authorization model IS the feature, not a hidden backend concern.

---

## Data Model Sketch

New tables (room-level, not platform-level):

```sql
-- Tier definitions per room
rooms__capability_tiers (
    id UUID PRIMARY KEY,
    room_id UUID REFERENCES rooms(id),
    name TEXT NOT NULL,           -- "Observer", "Participant", etc.
    capabilities JSONB NOT NULL,  -- ["vote", "propose", "configure"]
    gate JSONB NOT NULL,          -- constraint expression
    display_order INT,            -- for UX rendering
    UNIQUE(room_id, name)
)

-- Role assignments (for assigned_by gates)
rooms__role_assignments (
    id UUID PRIMARY KEY,
    room_id UUID REFERENCES rooms(id),
    account_id UUID REFERENCES accounts(id),
    tier_name TEXT NOT NULL,
    assigned_by UUID REFERENCES accounts(id),
    assigned_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(room_id, account_id, tier_name)
)
```

The endorsement table needs no schema change — out-of-slot endorsements are already storable (they just aren't created today because the creation path rejects at the slot limit).

---

## Open Questions

**Q33. Tier DAG rendering.** Tiers form a DAG, not a linear ladder. How do we render this without overwhelming non-technical users? Options: flatten to a linear progression for simple rooms (most cases), show full DAG only when parallel tracks exist.

**Q34. Tier inheritance.** Does holding "Contributor" automatically grant "Participant" capabilities? If tiers are a DAG this isn't automatic — you'd need to explicitly model `holds_tier(Participant)` as a gate for Contributor, or define inheritance separately. Linear rooms want inheritance; DAG rooms may not.

**Q35. Capability atomics.** What's the initial set of atomic capabilities? This is fuzzy and intentionally so — it will crystallize as rooms mature. Starting point: `read`, `vote`, `submit_evidence`, `propose_topic`, `flag_content`, `manage_polls`, `configure_room`, `assign_roles`.

**Q36. Gate evaluation cost.** Gates like `endorsed_by_count(tier, N)` require resolving which accounts hold a tier, then checking endorsement edges from those accounts. This is a join across tier resolution + endorsement queries. At what scale does this need caching?

---

## Implementation Path

| Phase | What | Depends on |
|-------|------|-----------|
| **1. Endorsement split** | Allow out-of-slot endorsements; add explicit slot selection UX | Endorsement table changes (allow overflow) |
| **2. Room tier config** | Define tiers per room; Owner tier auto-assigned | New tables, room config UI |
| **3. Tier resolution** | Compute which tiers a user holds; display in room UI | Gate evaluation engine |
| **4. Role assignment** | Owner can assign tiers to accounts | Assignment table, UI |
| **5. Elections** | Poll module produces tier assignments as output | Polling module extension |

Phase 1 is independently valuable (solves the invite problem) and can ship before the full tier system.

---

## Relationship to Existing Design Docs

- **Room types architecture (Q31):** Tiers are a *container-level* concept — they live in the room container, not the module. Any room module (polling, ranking, deliberation) inherits the same tier/capability system. The module defines which capabilities are meaningful for its interaction model.
- **Verifiers as graph participants (Q30):** The endorsement split resolves the slot budget problem. Verifiers issue out-of-slot endorsements; rooms gate on endorsement existence.
- **Trust engine on-demand computation (Q32):** Tier gates that reference trust scores (distance, diversity) benefit from on-demand computation — you only need scores for the specific user entering a specific room, not the full materialized snapshot.

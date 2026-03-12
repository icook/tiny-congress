# ADR-017: Two-Layer Trust Architecture — Platform Trust vs Communication Permission

## Status
Accepted

## Context

TinyCongress combines identity verification with deliberation rooms. Early design conflated these concerns — room access thresholds were baked into the trust engine, and the trust engine implicitly assumed it knew what rooms needed. This coupling creates problems:

- Rooms can't define custom access policies without modifying the trust engine.
- The trust engine carries room-specific logic (tier thresholds) that doesn't belong in an identity layer.
- Users can't reason about *why* they can or can't participate — is it because the platform doesn't trust them, or because this specific room has high requirements?

The system needs a clean separation between "are you a real human with a trustworthy identity?" and "does this particular room want to hear from you?"

## Decision

The trust system is split into two layers with distinct responsibilities:

### Layer 1: Platform Trust (Identity)

**The only hard gate is humanity.** The platform's job is to establish, with increasing confidence, that an account represents a unique biological human. This is Sybil resistance — nothing more.

- Handshakes **must** certify the claim of humanity.
- Handshakes **may** certify other public claims (attestations), but these are optional metadata on the trust edge, not requirements for platform participation.
- All humans are welcome, even untrustworthy ones. An untrustworthy human has poor graph position (high distance, low diversity), which limits their scope of impact — but they are never expelled from the platform for being untrustworthy.
- The trust engine computes graph metrics (distance, diversity, endorsement budget) and publishes them. It does not make access decisions.

**Platform trust governs:** endorsement slot count, daily action budget, and participation in the identity/trust network itself.

### Layer 2: Communication Permission (Rooms)

**Rooms are functionally isolated third-party services** that decide arbitrarily whether someone may participate. They consume the trust graph as a signal but define their own gating algorithms.

- A room might require `distance <= 3.0 AND diversity >= 2` (high trust).
- A room might require only `diversity >= 1` (any verified human).
- A room might use attestation metadata ("must have a physical QR handshake from a member").
- A room might ignore trust scores entirely and gate on some other criterion.

TinyCongress cannot know what a room thinks. The platform provides the identity infrastructure; rooms make their own judgments.

**Room admission is immediate.** When a user's trust graph position changes (e.g., after a new handshake), room eligibility updates in real-time. This contrasts with the trust engine's batch cadence (see ADR-021) — the graph recomputes on a schedule, but rooms evaluate the latest snapshot immediately.

### Architectural boundary

This maps to a provider/consumer pattern:

```
Platform Trust (Identity Provider)
  ├── Trust engine: computes distance, diversity, scores
  ├── Endorsement system: manages slots, budgets, handshakes
  ├── Publishes: daily trust graph snapshots
  └── Answers: "who is this person and what does the graph say about them?"

Room Service (Relying Party)
  ├── Consumes: trust graph snapshots
  ├── Defines: per-room gating policies
  ├── Evaluates: eligibility in real-time against latest snapshot
  └── Answers: "can this person participate here?"
```

The Room Service should be a separate service boundary, analogous to how the Verification Service is already split out.

## Consequences

### Positive
- Rooms can innovate on access policies without modifying the trust engine.
- The trust engine can evolve (new metrics, new algorithms) without breaking room policies.
- Users get clear feedback: "the platform trusts you at level X" vs "this room requires level Y."
- Third-party rooms become possible — external services can consume the trust graph API.

### Negative
- Two services to maintain instead of one integrated system.
- Room developers must understand the trust score API to write gating policies.
- Edge case: a room's policy might reference a trust metric that the platform hasn't computed yet (forward compatibility).

### Neutral
- Existing room constraint types (`EndorsedByConstraint`, `CommunityConstraint`, `CongressConstraint`) become reference implementations of room-side gating, not platform-level concepts.
- The current backend structure (`service/src/trust/constraints.rs`) will need to migrate to the room service boundary.

## Alternatives considered

### Single-layer model (trust engine gates rooms directly)
- Simpler — one system, one set of thresholds.
- Rejected because it couples identity verification to deliberation policy. Every new room type requires trust engine changes. Rooms can't experiment with access policies independently.

### Room-only trust (no platform layer)
- Each room maintains its own trust graph and verification.
- Rejected because it duplicates Sybil resistance work across every room and makes cross-room identity impossible.

## References
- [ADR-015: Identity model](015-identity-model.md) — the cryptographic identity layer this builds on
- [ADR-021: Batch reconciliation](021-batch-reconciliation.md) — the cadence at which platform trust updates
- TRD §3 (Trust Engine) and §7 (MVP Scope) — original design context

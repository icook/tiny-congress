# ADR-018: Handshake Protocol and Trust Edge Creation

## Status
Accepted

## Context

TinyCongress replaces centralized KYC (Stripe Identity, ID.me) with a peer-to-peer trust model. The fundamental question is: how does one human certify that another human is real?

Traditional identity verification stores PII (government IDs, biometrics, photographs) on a server. This creates a honeypot — a single breach exposes every user's identity documents. It also creates a dependency on a centralized verifier whose availability, pricing, and policies are outside the platform's control.

The system needs an identity verification mechanism that:
- Never stores PII on the server
- Produces a weighted trust signal (not binary yes/no)
- Works across different verification contexts (in-person, remote, referral)
- Creates a directed graph of trust relationships suitable for algorithmic analysis

## Decision

### The handshake as atomic unit

A **handshake** is the atomic unit of trust creation. It produces a `trust_edge` record linking two identities in the graph. The handshake certifies: "I, the voucher, attest that the vouchee is a real human."

Handshakes **must** certify humanity. Handshakes **may** additionally certify other public claims via attestation metadata (e.g., "I attest this person lives in Kansas City"), but the humanity claim is the required minimum.

### Handshake contexts and weights

The verification context determines the edge weight. Higher-friction verification produces higher-weight edges (shorter trust distance).

| Context | Mechanism | Weight | Rationale |
|---------|-----------|--------|-----------|
| **Physical QR** | User A generates a short-lived signed token encoded as QR. User B scans in person. Backend validates, creates edge. | 1.0 | Highest assurance — co-location proves physical presence. Hardest to fake at scale. |
| **Synchronous Remote** | Users conduct a live video call outside the app. User A views evidence and clicks "I attest this person is real." | 0.7 | Strong but not physical. Deepfakes are a theoretical risk at scale but impractical for targeted attacks on a small network. |
| **Social Referral** | User A sends an invite link. User B registers. A trust edge is created automatically. | 0.3 | Lowest friction, lowest assurance. Suitable for bootstrapping. The low weight means referral-only users have high trust distance, limiting their reach. |

Weight values are tuning parameters. The relative ordering (physical > remote > referral) is the decision; exact values will be calibrated based on observed graph topology.

### Zero-PII invariant

**The database never stores government IDs, photographs, biometric data, or any personally identifiable documents.** The system records attestations of trust, not evidence of identity. All evidence evaluation happens client-side or out-of-band (e.g., during a video call that the platform never sees).

The attestation metadata (JSONB on the trust edge) captures the *nature* of the verification ("physical_qr", "video_call") and optional structured claims, but never the raw evidence.

### Trust edge schema

Trust edges are stored in `reputation__endorsements` with the following relevant fields:

| Field | Purpose |
|-------|---------|
| `endorser_id` | The voucher (who is extending trust) |
| `subject_id` | The vouchee (who is receiving trust) |
| `topic` | Edge topic — trust graph traversal uses `"trust"` topic |
| `weight` | Edge weight in (0.0, 1.0], determined by handshake context |
| `attestation` | JSONB metadata: method, relationship context, confidence |
| `influence_staked` | Amount of endorser's influence locked on this edge (legacy — ADR-020 replaces continuous influence with discrete endorsement slots) |
| `revoked_at` | Non-null if the endorser revoked this edge |

Edges are directed: Alice endorsing Bob does not imply Bob endorsing Alice. Mutual trust requires two separate handshakes.

A unique constraint on `(subject_id, topic, endorser_id)` prevents duplicate active edges between the same pair for the same topic.

### Handshake flow: Physical QR

1. User A opens the app and requests a handshake QR code.
2. The app generates a short-lived JWT containing `{user_id, nonce, timestamp}`, signed with a per-user device key.
3. The JWT is encoded as a QR code displayed on User A's screen.
4. User B scans the QR code with their device.
5. User B's app submits the JWT to the backend along with their own signed attestation.
6. The backend validates: JWT signature, expiry, nonce uniqueness, both users exist.
7. A trust edge is created with `weight = 1.0` and `attestation = {"method": "physical_qr"}`.

The JWT TTL is short (5 minutes) to ensure temporal proximity. The nonce prevents replay.

### Handshake flow: Social Referral

1. User A generates an invite from the app.
2. The backend creates an invite record with a unique token and delivery metadata.
3. User A shares the invite link (via text, email, etc. — out of band).
4. User B opens the link, registers a new account.
5. User B accepts the invite.
6. A trust edge is created with `weight = 0.3` and `attestation = {"method": "social_referral"}`.

The invite is single-use. The low weight ensures referral-only paths are long in the trust graph.

## Consequences

### Positive
- No PII storage eliminates the identity document honeypot risk.
- Weighted edges create a natural quality gradient — the system structurally values high-friction verification over low-friction referrals.
- The directed graph supports asymmetric trust (Alice trusts Bob but Bob hasn't verified Alice).
- Multiple handshake types allow users to choose their comfort level — in-person for high trust, referral for convenience.

### Negative
- The system cannot *prove* humanity — it records *attestations* of humanity. A sufficiently motivated attacker can complete a physical QR handshake with a fake identity.
- Weight values are subjective. "0.7 for video call" is an estimate, not a derivation.
- Social referral at weight 0.3 may be too generous or too restrictive — calibration requires real usage data.

### Neutral
- The attestation JSONB field is intentionally unstructured. It can carry arbitrary metadata without schema changes, at the cost of no compile-time validation of its contents.
- Genesis endorsements (bootstrapping the first user) have `endorser_id IS NULL` and do not participate in graph traversal. This is a special case for network initialization.

## Alternatives considered

### Centralized KYC provider (Stripe Identity, ID.me)
- Turnkey solution with strong identity assurance.
- Rejected because it creates a dependency on a third-party provider, stores PII (or delegates PII storage), and doesn't produce a trust graph suitable for Sybil resistance analysis.
- May be offered as an optional high-weight handshake context in the future (the system can treat "KYC verified" as another attestation type).

### Binary trust (verified / not verified)
- Simpler model — no weights, no distance computation.
- Rejected because it loses the quality gradient. A social referral would carry the same trust as an in-person handshake, eliminating the incentive for higher-friction verification.

### Bidirectional edges (mutual trust from a single handshake)
- A handshake between Alice and Bob creates edges in both directions.
- Rejected because it conflates "I vouch for you" with "you vouch for me." A QR scan proves Alice showed up, but Bob might have been coerced. Asymmetric edges preserve the distinction.

## References
- [ADR-008: Account-based verifiers](008-account-based-verifiers.md) — verifier endorsements write to the same `reputation__endorsements` table
- [ADR-015: Identity model](015-identity-model.md) — the cryptographic keys used to sign handshake tokens
- [ADR-017: Two-layer trust architecture](017-two-layer-trust-architecture.md) — handshakes serve the platform trust layer
- TRD §2 (Identity & Handshake Protocol) — original specification
- `service/src/trust/repo/invites.rs` — invite (social referral) implementation
- `service/src/trust/http/mod.rs` — trust endpoint handlers

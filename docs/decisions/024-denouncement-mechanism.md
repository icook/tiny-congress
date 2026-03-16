# ADR-024: Denouncement Mechanism — Denouncer-Only Edge Revocation

## Status
Accepted (2026-03-13)

**Expanded 2026-03-16 (ADR-029):** Added Propagation section documenting the one-hop cascade parameters, penalty operating point, and loss function bias. These were validated by simulation sweep across all adversarial topologies (PR #678).

## Context

ADR-020 established denouncement budgets (d=2) but explicitly deferred the mechanism — what happens to the trust graph when a denouncement is recorded. The trust graph simulation framework (PR #643, GitHub #624) tested three candidate mechanisms against adversarial topologies to determine which best balances effectiveness against bad actors with resistance to weaponization.

### Mechanisms tested

| Mechanism | How it works | Removes bad actors? | Weaponization-resistant? |
|---|---|---|---|
| Edge removal (nuclear) | Revokes ALL inbound edges to target | Yes — target becomes unreachable | Yes for target, but trivially weaponized: one denouncement = total disconnection |
| Score penalty | Adds distance penalty + reduces diversity on target's snapshot | Yes — degrades scores below eligibility | No — stacks linearly; 10 coordinated denouncements = 30.0 penalty overwhelms any legitimate score |
| Sponsorship cascade | Revokes endorser→target edges + penalizes endorsers | Partially — endorsers punished, target loses paths | Yes — penalties hit endorsers, not target directly |

The simulation's weaponization test confirmed that score penalty fails: a coordinated group can destroy a legitimate user's scores. Edge removal is effective but disproportionate — a single denouncement erases someone entirely. Sponsorship cascade is the most interesting but its collateral damage (1/7 blue nodes affected in mercenary scenario) and complexity make it better suited as a propagation mechanism than a primary response.

## Decision

### Denouncer-only edge revocation is the baseline mechanism.

When user A denounces user B, the system revokes A's endorsement edge to B. The effect is: "I no longer vouch for this person."

- A user cannot simultaneously endorse and denounce the same person
- Each denouncement consumes 1 denouncement budget (d=2 per ADR-020)
- Only the denouncer's own edge is affected — other users' endorsements of the target remain intact
- The target loses one inbound path but remains reachable via other endorsers

This is the proportionate, non-weaponizable response. A single bad-faith denouncement costs the target one path, not all of them. A coordinated attack by N users costs each attacker a denouncement budget slot and only removes N edges — the target's other endorsers are unaffected.

### Severe action requires adjudication, not automation.

Full disconnection (revoking all edges) or slashing is too consequential for an automated threshold. The appropriate mechanism for severe cases is a governance process:

- A motion is raised to slash the target
- Evidence is presented
- Broad consensus from a diverse, deeply trusted quorum is required
- The threshold should be set conservatively — essentially "the graph is in consensus"

This is a substantial design problem and will be addressed in a separate ADR when the governance model is designed.

## Propagation (Sponsorship Cascade)

Endorsing someone who later gets denounced carries consequences. This is the "sponsorship risk" principle from ADR-020: you stake your reputation when you vouch for someone.

### One-hop cascade only.

When user B is denounced, B's endorsers (users who endorsed B) receive a penalty. The cascade does not propagate further — a denouncement against Bob penalizes Alice (who endorsed Bob) but not Carol (who endorsed Alice). This was tested explicitly; no propagation occurs beyond direct endorsers.

### Fixed penalty: 2.0 distance / -1 diversity.

Endorsers of a denounced user receive a +2.0 distance penalty and -1 diversity reduction. This is lighter than the primary denouncement effect (edge revocation removes a full path) and represents the cost of poor judgment in endorsement, not direct complicity.

### Operating point selected via sweep.

A penalty sweep from 1.0 through 4.0 was run against all adversarial topologies (hub-and-spoke Sybil, mercenary bot network, colluding ring). Higher penalty values increase blue casualties (legitimate users caught in cascade) without meaningfully improving red blocking. The 2.0/-1 point minimizes the loss function: bad actors reliably lose eligibility while legitimate users caught in cascade retain a remediation path (seek a fresh endorsement from someone who actually knows them).

### Loss function bias: false negatives >> false positives.

The tuning bias is defensive. False negatives (bad actors retaining eligibility) are weighted much more costly than false positives (legitimate users temporarily downgraded). Rationale: early in the network, real-world consequences of being temporarily downgraded are minimal, and the remediation path is organic. Cascade collateral (e.g., 1/7 blue nodes in the mercenary scenario) is acceptable under this bias.

This asymmetry should narrow as the network matures and trust status carries real-world weight — the threshold shifts toward due process at scale.

### Circular cascade safety.

Tested on ring topology A->B->C->A. No runaway penalty accumulation. The engine does not follow cycles — each node is visited once per anchor computation.

## Consequences

### Positive
- **Non-weaponizable.** A single malicious actor can only revoke their own edge — no way to disconnect a legitimate user.
- **Proportionate.** The response scales with the number of independent denouncers, not with any single actor's influence.
- **Simple.** No score mutations, no cascade logic, no threshold parameters to tune. A denouncement is a revocation.
- **Composable.** Denouncement propagation (one-hop cascade, documented above) layers on top of this baseline without changing the core mechanism.

### Negative
- **May be too soft.** If a bad actor has many endorsers, losing one edge doesn't meaningfully degrade their scores. They remain reachable and potentially eligible. Mitigated by: time decay will naturally erode stale edges; denouncement propagation will penalize endorsers; adjudication handles severe cases.
- **Requires complementary mechanisms.** Denouncer-only revocation alone won't remove a well-connected bad actor. It composes with the one-hop cascade (documented above), time decay (ADR-025), and adjudication (future work) to complete the picture.
- **Cascade produces collateral.** The one-hop penalty affects legitimate endorsers of a denounced user. In the mercenary scenario, 1/7 blue nodes are temporarily affected. The loss function bias accepts this trade-off — remediation via fresh endorsement is available.

### Neutral
- ADR-020's denouncement budget (d=2) works naturally with this mechanism: each denouncement costs 1 budget and revokes 1 edge.
- The simulation harness should add `apply_denouncer_revocation(denouncer, target)` to validate effectiveness against adversarial topologies.

## Alternatives considered

### Nuclear edge removal (rejected)

Revoke all inbound edges to the target on any denouncement.

Rejected because a single denouncement completely disconnects the target — trivially weaponized. One malicious actor can erase a legitimate user from the trust graph.

### Score penalty (rejected)

Add distance penalty and reduce diversity on the target's score snapshot.

Rejected because penalties stack linearly and are trivially weaponizable by coordinated groups. The simulation's weaponization test confirmed this: 10 denouncements produce a 30.0 distance penalty that overwhelms any legitimate trust score.

### Automated threshold cascade (deferred)

Require N independent denouncements before triggering severe action (full disconnection).

Not rejected but reframed: the "right" version of this is adjudication with governance, not an automated counter. A threshold count doesn't capture whether denouncements represent genuine community consensus or a coordinated Sybil cluster. Deferred to a future ADR on governance slashing.

## References
- [ADR-020: Reputation scarcity](020-reputation-scarcity.md) — denouncement budget (d=2)
- [ADR-023: Fixed slots with variable weight](023-fixed-slot-variable-weight.md) — slot model for endorsements
- [ADR-019: Trust engine computation](019-trust-engine-computation.md) — distance/diversity scoring
- [ADR-025: Trust edge time decay](025-trust-edge-time-decay.md) — complementary passive Sybil resistance
- [GitHub #624: Trust graph simulation](https://github.com/icook/tiny-congress/issues/624) — adversarial topology testing
- PR #643: Simulation harness with mechanism comparison framework
- PR #678: Adversarial simulation suite — 31 scenarios, mechanism comparison, penalty sweep
- `.plan/2026-03-13-trust-robustness-overview.md` — section 3c (cascade parameters and loss function)
- `.plan/2026-03-13-open-questions.md` — Q16-Q19 (denouncement propagation)

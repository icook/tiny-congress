# ADR-025: Trust Edge Time Decay

## Status
Accepted (2026-03-13)

## Context

Trust edges in the graph currently have no temporal dimension — an endorsement created two years ago with no further interaction carries the same weight as one created yesterday. This doesn't reflect how trust works in practice: relationships that aren't maintained become stale, and the trust signal they carry degrades.

Time decay serves multiple purposes in the trust model:

- **Accuracy.** A two-year-old endorsement from someone you've lost touch with is a weaker signal than a recent one from an active relationship.
- **Natural Sybil resistance.** Fabricated edges created by a Sybil operator decay over time, narrowing the attack window. The attacker must continuously re-create edges to maintain graph position, increasing operational cost.
- **Graph hygiene.** Without decay, the graph accumulates stale edges indefinitely. Decay naturally prunes inactive relationships without requiring explicit revocation.

Three candidate decay functions were evaluated in the trust simulation harness against adversarial topologies:

| Function | Formula | Behavior |
|---|---|---|
| Exponential (6-month half-life) | `weight × 0.5^(age_days / 180)` | Smooth, continuous. After 6mo: 0.50, 1yr: 0.25, 2yr: 0.06 |
| Step (annual tiers) | `1.0` for <1yr, `0.5` for 1-2yr, `0.0` for >2yr | Discrete jumps. Full weight for a year, then halves, then expires |
| Linear (2-year decline) | `max(0, 1 - age_days/730)` | Steady decline. After 6mo: 0.75, 1yr: 0.50, 18mo: 0.25, 2yr: 0.00 |

## Decision

### Step function is the recommended decay model.

The step function (`1.0` for year 1, `0.5` for year 2, `0.0` after year 2) is selected based on simulation evidence:

**Legitimate network preservation.** With 4 independent endorsers and CommunityConstraint(5.0, 2):
- Step function: eligible through 18 months (weight=0.5, diversity=4, distance=3.0)
- Exponential: eligible at 6 months (weight=0.5) but FAILS at 12 months (distance=5.08 > 5.0)
- Linear: eligible at 12 months (weight=0.5) but FAILS at 18 months (distance=5.01 > 5.0)

Step gives users the longest grace period before renewal pressure kicks in. An endorsement is fully valid for a year, then weakens, then expires. This is the most user-friendly model — predictable, easy to communicate, and forgiving.

**Sybil defense is from diversity, not decay.** All three functions showed the same result: Sybil spokes NEVER achieve diversity ≥ 2 at any edge age (tested 30d through 730d). The diversity metric is the primary Sybil barrier. Decay is complementary — it auto-releases stale Sybil edges after 2 years, reducing graph clutter. Since all three functions provide equivalent Sybil defense, the choice is driven entirely by UX impact on legitimate users.

### Renewal mechanism: re-swap resets the decay clock.

Users renew an endorsement by re-performing the handshake (QR code, video call, etc.). This resets the creation timestamp and can also upgrade the weight (e.g., text → QR as trust deepens). No new UX surface is needed — the existing swap flow handles renewal.

### Slot auto-release below weight floor of 0.05.

Edges whose effective weight drops below 0.05 are auto-released, freeing the endorsement slot. Simulation evidence:

- Auto-releasing a near-zero edge (weight=0.01) changed diversity by at most 1 and did not change eligibility
- Under step function: edges auto-release at the 2-year boundary (weight drops from 0.5 → 0.0)
- Under all functions: the freshest edge (1 week old) retains >95% weight, so recent endorsements are never at risk

Auto-release is safe because near-zero edges contribute negligibly to trust scores. Manual revocation would burden users with managing dead slots.

### Engine integration: batch reconciliation (ADR-021).

Decay is applied during periodic batch reconciliation, not at query time. Rationale:
- Aligns with existing reconciliation infrastructure
- Avoids adding temporal computation to every trust query
- The step function's discrete tiers mean staleness between reconciliation runs has minimal impact (weight is either 1.0, 0.5, or 0.0 — not a smooth curve)
- Reconciliation frequency (daily or weekly) is sufficient for annual-tier boundaries

## Simulation Evidence

9 simulation scenarios validated this decision across adversarial topologies. Key results:

### Sybil attack window (Step 11)

| Decay | Edge Age | Sybil Weight | Spoke Diversity | Spoke Distance | Eligible? |
|---|---|---|---|---|---|
| step | 30d | 1.00 | 1 | 5.33 | NO |
| step | 180d | 1.00 | 1 | 5.33 | NO |
| step | 365d | 0.50 | 1 | 9.67 | NO |
| step | 730d | 0.00 | 0 | — (auto-released) | NO |
| exponential | 30d | 0.89 | 1 | 5.86 | NO |
| exponential | 180d | 0.50 | 1 | 9.67 | NO |
| exponential | 365d | 0.25 | 0 | — | NO |
| exponential | 730d | 0.06 | 0 | — | NO |
| linear | 30d | 0.96 | 1 | 5.52 | NO |
| linear | 180d | 0.75 | 1 | 6.75 | NO |
| linear | 365d | 0.50 | 1 | 9.67 | NO |
| linear | 730d | 0.00 | 0 | — (auto-released) | NO |

Sybil spokes are NEVER eligible regardless of edge age or decay function. Diversity=1 is the blocking factor, not weight.

### Stale legitimate edges (Step 12)

Topology: anchor → 4 independent endorsers → hub (all endorser→hub edges at the same age)

| Decay | Edge Age | Decayed Weight | Hub Diversity | Hub Distance | Eligible? |
|---|---|---|---|---|---|
| **step** | **180d** | **1.00** | **4** | **2.00** | **YES** |
| **step** | **365d** | **0.50** | **4** | **3.00** | **YES** |
| **step** | **548d** | **0.50** | **4** | **3.00** | **YES** |
| step | 730d | 0.00 | 0 | — | NO (auto-released) |
| exponential | 180d | 0.50 | 4 | 3.00 | YES |
| exponential | 365d | 0.25 | 4 | 5.08 | NO |
| exponential | 548d | 0.12 | 4 | 9.25 | NO |
| linear | 180d | 0.75 | 4 | 2.33 | YES |
| linear | 365d | 0.50 | 4 | 3.00 | YES |
| linear | 548d | 0.25 | 4 | 5.01 | NO |

Step function preserves eligibility through 18 months — the longest of any function. The discrete step at year 1 (1.0 → 0.5) is gentle enough that distance stays within bounds. Exponential and linear cause distance to exceed 5.0 threshold much earlier.

### Mixed-age network (Step 12b)

Topology: target with 2 fresh endorsers (7d, 30d) and 2 stale endorsers (548d, 912d)

| Decay | Target Diversity | Target Distance | Eligible? |
|---|---|---|---|
| exponential | 4 | 2.03 | YES |
| step | 3 | 2.00 | YES |
| linear | 3 | 2.01 | YES |

All functions: target remains eligible with mixed-age endorsements. Fresh endorsements carry enough weight to maintain eligibility even when stale ones contribute little or auto-release.

### Slot auto-release (Step 13)

10 edges, ages evenly spaced from 7 days to 3 years:

| Decay | Dead (<0.05) | Weak (0.05-0.2) | Active (≥0.2) |
|---|---|---|---|
| exponential | 3 | 3 | 4 |
| step | 4 | 0 | 6 |
| linear | 4 | 1 | 5 |

Step function produces the cleanest distribution: edges are either fully active (weight ≥ 0.5 in year 1-2) or fully dead (weight = 0.0 after year 2). No ambiguous "weak" edges that might or might not contribute.

### Auto-release score impact (Step 13b)

| State | Diversity | Distance | Eligible? |
|---|---|---|---|
| With stale edge (weight=0.01) | 3 | 1.0 | YES |
| Without stale edge (auto-released) | 2 | 1.0 | YES |

Auto-release is safe: eligibility unchanged, diversity shifted by 1 (acceptable).

### Decay function comparison (separation power)

The ratio of legitimate edge weight (1 week old) to Sybil edge weight (aged) measures "separation power":

| Function | 6 months | 1 year | 2 years |
|---|---|---|---|
| Exponential | 1.9x | 4.0x | 16.2x |
| Step | 1.0x | 2.0x | ∞ |
| Linear | 1.3x | 2.0x | ∞ |

Exponential has the best continuous separation. Step has no separation in year 1 but infinite separation after year 2 (Sybil weight drops to zero). For the trust model, binary separation (eligible or not) matters more than continuous weight ratios.

## Consequences

### Positive
- **Passive Sybil resistance.** Attack edges auto-release after 2 years without any user action.
- **Graph accuracy.** Edge weights reflect current relationship state with a predictable annual cadence.
- **Composable with denouncement.** Decay + denouncer-only revocation (ADR-024) means bad actors face both active removal (denouncement) and passive erosion (decay).
- **User-friendly.** "Your endorsement is good for a year, then weakens, then expires after two years" is easy to communicate. Renewal = re-swap.
- **Clean slot management.** Auto-release at the 2-year mark frees slots without user action. No ambiguous "weak" edges cluttering slot budgets.

### Negative
- **Annual renewal pressure.** Users should re-swap within 2 years to maintain endorsements. If a user goes inactive for 2+ years, all their outbound endorsements expire.
- **Cliff at 2 years.** The step function's discrete boundary means weight drops from 0.5 → 0.0 overnight. A user whose endorsements all hit the 2-year mark simultaneously could lose standing abruptly. Mitigated by: mixed-age endorsements are the norm (users don't endorse everyone on the same day); the frontend can warn when endorsements approach expiry.
- **Batch staleness.** Between reconciliation runs, weights may be stale. With daily reconciliation and annual-tier boundaries, this is negligible.

### Neutral
- Interacts with ADR-023's weight model: decay modifies effective weight over time, adding a third dimension beyond swap method and relationship depth.
- Denouncement propagation impact: if Alice's endorsement of Bob has decayed to 0.5 (year 2), she's still penalized when Bob is denounced — the cascade penalty is independent of edge weight. This is intentional: vouching for someone carries risk regardless of how recent the endorsement is.

## References
- [ADR-024: Denouncement mechanism](024-denouncement-mechanism.md) — denouncer-only revocation as baseline
- [ADR-023: Fixed slots with variable weight](023-fixed-slot-variable-weight.md) — weight model that decay modifies
- [ADR-021: Batch reconciliation](021-batch-reconciliation.md) — execution model for periodic decay
- [ADR-020: Reputation scarcity](020-reputation-scarcity.md) — endorsement slots (k=10) and denouncement budget (d=2)
- [GitHub #624: Trust graph simulation](https://github.com/icook/tiny-congress/issues/624) — simulation harness
- PR #673: GraphSpec extraction with temporal extensions
- `service/tests/trust_simulation_tests.rs`: 9 temporal simulation scenarios validating this decision

# Scale Analysis Findings

**Date:** 2026-03-13
**Branch:** test/680-scale-simulation-framework
**Tickets:** #680, #681, #682

---

## Scale confidence assessment

| Scale | Confidence | Nature of work | Reasoning |
|---|---|---|---|
| 1k–5k users | **High** | Build and verify (completable) | Mechanism properties are scale-invariant. Organic growth produces shallow graphs (most users within 3-4 hops). Batch reconciliation trivially fast. |
| 5k–10k users | **Medium** | Build, verify, instrument (completable) | Distance stays low (BA mean ~2.6 at 2k). Diversity computation is the bottleneck — O(n²) dense matrix in FlowGraph hits memory wall. Need sparse max-flow + monitoring. |
| 10k–100k users | **Low-Medium** | Ongoing operations (never done) | Mechanism math is sound. But engine performance, realistic topology, sophisticated Sybil strategies, and novel attack vectors are ongoing challenges. Requires detection, response, and adaptation — not just more testing. |

**Note:** "Low-Medium" at 100k is not a problem to solve but the nature of adversarial systems at scale. No deployed trust system (PageRank, Bitcoin, PKI) was "proven robust" pre-launch. Confidence improves with operational experience but never reaches certainty.

## Key findings from scale simulation

### 1. BA graph structure at 1k–2k nodes

- **100% reachable** — all nodes connected (BA property, m=3)
- **Distance distribution**: mean=2.5, max=4.0. Well within the 5.0 eligibility threshold.
- **Diversity distribution**: min=3 at 1k nodes. BA graphs with m=3 produce high connectivity — every non-anchor node has at least 3 vertex-disjoint paths.
- **Implication**: at k=10 endorsement slots and m≈3 average endorsements, the graph is robustly connected. The 5.0 distance threshold is very generous — could be tightened.

### 2. Sybil mesh diversity = bridge count

**Critical finding:** Sybil mesh members achieve diversity exactly equal to the number of compromised bridge nodes on vertex-disjoint paths from anchor.

| Bridge count | Max Sybil diversity | Passes threshold (≥2)? |
|---|---|---|
| 1 | 1 | NO |
| 2 | 2 | YES |
| 3 | 3 | YES |
| 5 | 5 | YES |

**What this means:**
- The system's Sybil resistance reduces to: "how hard is it to compromise 2+ accounts on genuinely independent paths?"
- The mesh itself doesn't inflate diversity — internal fake-to-fake endorsements don't create new independent paths from anchor.
- At small scale (<1k), compromising 2 independent accounts is meaningfully hard (requires real social engineering of 2 separate people).
- At large scale (100k+), the attack surface grows. Need to evaluate how many accounts are "cheaply" compromisable and whether they tend to lie on independent paths.

### 3. Engine architecture bottleneck: FlowGraph

The current `FlowGraph` (diversity computation) uses `Vec<Vec<i32>>` — a dense O(n²) capacity matrix:
- At n=100: 40 KB → trivial
- At n=1,000: 4 MB → fine
- At n=5,000: 100 MB → tight
- At n=10,000: 400 MB → painful
- At n=100,000: ~40 GB → impossible

The scale simulation framework implements a **sparse Edmonds-Karp** using `HashMap<(usize, usize), i32>` for residual capacities. This uses O(E) space and handles 100k+ nodes.

**Action needed:** The engine's `FlowGraph` must be replaced with a sparse implementation before scaling beyond ~5k users. This is an engine change, not a mechanism change — the algorithm's output is identical.

### 4. Bridge removal resilience (BA graphs)

Removing the 3 highest-degree nodes from a 1k-node BA graph:
- After 1 removal: 99.9% still reachable
- After 3 removals: 99.7% still reachable

BA graphs are highly resilient to targeted node removal due to their multiple-path topology. This is good — a single bridge node failure doesn't fragment the graph.

### 5. Correlated decay is localized

Marking 100 cohort nodes' internal edges as 2+ years old (triggering step decay to weight=0.01):
- Only 3 nodes got increased distance
- 0 nodes became unreachable
- Graph stayed 100% reachable

BA graph topology provides redundant paths: even when all intra-cohort edges decay, each cohort member still has edges to non-cohort nodes from the BA construction. The correlated failure is absorbed by the graph's redundancy.

**Caveat:** This tests BA topology, not real social graphs. In real networks, cohort members might have most/all of their endorsements within the cohort (e.g., a tight-knit community). The damage would be worse.

---

## Open questions for next phase

1. **Real topology modeling.** BA produces unrealistically high connectivity (every node has m=3+ independent paths). Real social graphs have much more clustering and fewer independent paths. Need community-structure generators (e.g., stochastic block model) to test more realistic topologies.

2. **Engine sparse max-flow migration.** When should this happen? Before or after launch? The current dense implementation works fine at demo scale (~100 users). The sparse implementation is proven in tests — could be ported to the engine incrementally.

3. **Sybil mesh countermeasures.** With diversity=bridge_count as a proven relationship, what additional mechanisms could detect mesh attacks beyond the diversity threshold? Options: temporal analysis (did all endorsements happen simultaneously?), graph structure analysis (is there a dense cluster with few external connections?), behavioral signals.

4. **Community-structure testing.** Generate graphs with the Stochastic Block Model (dense intra-community, sparse inter-community) and re-run all scale tests. This would surface whether the current thresholds work for realistic social structure or need adjustment.

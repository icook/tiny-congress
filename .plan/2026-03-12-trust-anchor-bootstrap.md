# Trust Anchor Bootstrap — Implementation Plan

**Date:** 2026-03-12
**Issue:** #638
**ADR:** 019-trust-engine-computation.md

## Decision

Engine-level injection (Option C). The `compute_distances_from(anchor_id)` function explicitly injects `(anchor_id, 0.0)` into the result set. The CTE handles graph traversal; the engine handles the boundary condition.

## Why this approach

- Convention row (Option A): fragile — batch recomputation could overwrite it
- Self-endorsement (Option B): semantically wrong, consumes a slot, produces distance=1.0
- Genesis endorsement (Option D): requires CTE modification for NULL endorser handling
- Engine injection (Option C): one-liner in the right place, no schema changes

## Implementation steps

1. **Read `compute_distances_from` in `service/src/trust/engine.rs`** — understand current return type and flow
2. **Inject anchor score** — after CTE results are collected, insert/prepend `(anchor_id, distance=0.0)` into the result set
3. **Handle path_diversity for anchor** — the anchor's diversity should be set to a high value (it's the root of trust, not a node needing independent verification). Use `i32::MAX` or a sentinel value.
4. **Ensure `upsert_score` handles anchor** — when persisting to `trust__score_snapshots`, the anchor row should be created/updated like any other score
5. **Batch recomputation** — verify that `recompute_from_anchor` preserves the anchor's own score (it should, since we inject it every time)
6. **Add test** — new test in `trust_engine_tests.rs`: create an anchor, run `compute_distances_from(anchor)`, assert anchor has distance=0.0

## Scope

- IN: anchor distance injection, anchor diversity handling, test
- OUT: multiple anchor support (future), anchor selection UI, genesis endorsement changes

## Estimated effort

~1-2 hours. Likely 5-10 lines of engine code + 20-30 lines of test.

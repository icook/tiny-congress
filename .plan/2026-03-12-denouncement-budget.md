# Denouncement Budget Refactor — Implementation Plan

**Date:** 2026-03-12
**Issue:** #637
**ADR:** 020-reputation-scarcity.md

## Decision

Replace influence-cost denouncement with a permanent budget of d=2. Each user can file at most 2 denouncements, ever. These are non-refundable — you cannot retract a denouncement to get the slot back.

## Key constraints

- Denouncements are recorded but **do not affect trust graph traversal** (ADR-020). The graph effect is a separate design question (see `.plan/2026-03-12-sponsorship-risk-design.md` on `test/624-trust-graph-simulation`).
- Denouncements are independent of endorsement slots — filing a denouncement does not reduce your endorsement capacity.
- The d=2 limit is permanent, not daily. It does NOT regenerate with the daily action budget.

## Implementation steps

1. **Read current denouncement flow** — trace from `POST /trust/denounce` through service layer to repo. Identify where `influence_cost` is checked/deducted.
2. **Replace validation** — change from "available influence >= denouncement cost" to "active denouncement count < 2".
3. **Remove influence deduction** — denouncement should not modify influence/budget state.
4. **Ensure non-refundable** — verify there's no "un-denounce" path. If revocation exists, remove it or gate it.
5. **Update response shape** — if the budget endpoint reports denouncement capacity, update it to show `{denouncements_total: 2, denouncements_used: N, denouncements_available: 2-N}`.
6. **Update tests** — modify existing denouncement tests to use count-based validation.

## Scope

- IN: budget validation change, remove influence cost, update tests
- OUT: denouncement graph effect (future), denouncement UI, denouncement retraction

## Estimated effort

~1-2 hours. Focused refactor of validation logic + test updates.

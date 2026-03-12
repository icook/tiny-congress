# Endorsement Slots Refactor — Implementation Plan

**Date:** 2026-03-12
**Issue:** #636
**ADR:** 020-reputation-scarcity.md

## Decision

Replace the continuous influence budget (default 10.0, float-valued pool with staked/spent tracking) with discrete endorsement slots. Each user has k slots. Each active (non-revoked) endorsement occupies one slot. When all slots are occupied, the user must revoke an existing endorsement before creating a new one.

- Demo: k=3
- Production: k=5
- Verifier/platform accounts: unlimited (exempt from slot limits per ADR-020)

## Current state (what to change)

### Database
- `trust__user_influence` table: tracks `total_influence`, `staked_influence`, `spent_influence` per user
- `reputation__endorsements.influence_staked`: float column tracking influence locked per endorsement
- Budget check: `available = total - staked - spent >= endorsement_cost`

### Code
- `service/src/trust/repo/influence.rs` — influence CRUD (get_budget, stake_influence, unstake_influence, spend_influence)
- `service/src/trust/service.rs` — endorsement creation checks influence availability
- `service/src/trust/http/mod.rs` — `GET /trust/budget` returns BudgetResponse with float fields

## Implementation steps

### Phase 1: Slot counting (minimal change)
1. **Add slot config** — constant `ENDORSEMENT_SLOTS_DEMO: u32 = 3` and `ENDORSEMENT_SLOTS_PROD: u32 = 5`. Use feature flag or config to switch.
2. **Replace endorsement validation** — in service layer, change from influence check to: `SELECT COUNT(*) FROM reputation__endorsements WHERE endorser_id = $1 AND revoked_at IS NULL` < k.
3. **Replace revocation logic** — revoke should not "return influence"; it just means the count decreases by 1, freeing a slot.
4. **Verifier exemption** — check if account has `authorized_verifier` endorsement; if so, skip slot check.
5. **Update budget endpoint** — `GET /trust/budget` returns `{slots_total: k, slots_used: count, slots_available: k - count}`.

### Phase 2: Schema cleanup
6. **Migration: drop or repurpose `influence_staked`** — if repurposing, set to 1.0 for all active endorsements (boolean-ish). If dropping, remove the column.
7. **Migration: drop or deprecate `trust__user_influence`** — if slot count is derived from config + count query, this table may be unnecessary. Evaluate whether it's used for anything else.
8. **Remove influence repo methods** — `stake_influence`, `unstake_influence`, `spend_influence` become dead code under the slot model.

### Phase 3: Test updates
9. **Update existing tests** — any test that sets up influence budgets or checks influence values needs to use slot model.
10. **Add slot-specific tests:**
    - Endorse up to k, verify k+1 is rejected
    - Revoke one, verify endorsement succeeds again
    - Verifier account can endorse beyond k
    - Slot count is correct after mixed endorse/revoke operations

## Open questions for implementer

- **`trust__user_influence` table fate:** Drop entirely, or keep for future use (dynamic slot count based on trust score)? ADR-020 mentions "platform trust level may increase slot count" — if we keep the table, it could store the dynamic slot allocation. If we drop it, we'd need to re-add something later. **Recommendation:** Keep the table but repurpose columns: `total_influence` → `slot_count` (integer), drop staked/spent.
- **Action budget:** ADR-020 also defines a daily renewable action budget (1-3 for demo). This is separate from slots (capacity vs rate limit). NOT in scope for this ticket — will be a separate issue if needed.

## Scope

- IN: slot validation, budget endpoint, verifier exemption, schema cleanup, test updates
- OUT: daily action budget, batch reconciliation, dynamic slot allocation, UI changes

## Estimated effort

Effort/medium (~4-8 hours). The validation swap is straightforward but the schema cleanup and test updates touch multiple files.

## Dependencies

- #638 (trust anchor bootstrap) should land first so trust score tests have a working baseline
- Independent of #637 (denouncement budget) — can be parallelized

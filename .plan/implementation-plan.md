# Weight Selection UI Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Let users choose endorsement strength (delivery method × relationship depth) when creating invites, replacing the hardcoded weight=1.0.

**Architecture:** Add `relationship_depth` and `weight` columns to `trust__invites`, expand the `delivery_method` CHECK constraint, compute weight on the frontend, pass it through invite creation → acceptance → endorsement. Direct `POST /trust/endorse` already accepts weight, so only the invite-based flow needs changes.

**Tech Stack:** Rust (axum, sqlx), React (Mantine), SQL migration

---

### Task 1: Database Migration — Expand Invite Schema

**Files:**
- Create: `service/migrations/18_invite_weight_columns.sql`

**Step 1: Write the migration**

```sql
-- Expand delivery_method to include video and text
ALTER TABLE trust__invites DROP CONSTRAINT IF EXISTS trust__invites_delivery_method_check;
ALTER TABLE trust__invites ADD CONSTRAINT trust__invites_delivery_method_check
    CHECK (delivery_method IN ('qr', 'video', 'text', 'email'));

-- Add relationship depth and computed weight
ALTER TABLE trust__invites ADD COLUMN relationship_depth TEXT NOT NULL DEFAULT 'acquaintance'
    CHECK (relationship_depth IN ('years', 'months', 'acquaintance'));
ALTER TABLE trust__invites ADD COLUMN weight REAL NOT NULL DEFAULT 1.0
    CHECK (weight > 0.0 AND weight <= 1.0);
```

**Step 2: Verify migration applies cleanly**

Run: `cargo test --test trust_invite_tests -- --nocapture 2>&1 | head -20`
Expected: Tests still pass (testcontainers runs all migrations).

**Step 3: Commit**

```bash
git add service/migrations/18_invite_weight_columns.sql
git commit -m "feat(trust): add weight and relationship_depth columns to invites (#656)"
```

---

### Task 2: Backend — Update Invite Repo Layer

**Files:**
- Modify: `service/src/trust/repo/invites.rs` (the `create_invite` function, ~line 15)
- Modify: `service/src/trust/http/mod.rs` (invite-related request/response types)

**Step 1: Write the failing test**

Add to `service/tests/trust_invite_tests.rs`:

```rust
#[tokio::test]
async fn create_invite_with_weight_fields() {
    // Setup: create test account, get trust repo
    // Call create_invite with delivery_method="video", relationship_depth="months"
    // Assert: returned invite has weight = 0.7 * 0.7 = 0.49
    // Assert: invite row in DB has correct delivery_method, relationship_depth, weight
}
```

The test should call `create_invite` with `delivery_method = "video"` and `relationship_depth = "months"`, then verify the stored `weight` is `0.49` (0.7 × 0.7).

**Step 2: Run test to verify it fails**

Run: `cargo test --test trust_invite_tests create_invite_with_weight_fields -- --nocapture`
Expected: FAIL — `create_invite` doesn't accept the new parameters yet.

**Step 3: Update the repo function**

In `service/src/trust/repo/invites.rs`, update `create_invite` to:
- Accept `relationship_depth: &str` parameter
- Compute weight from the delivery_method × relationship_depth table:

```rust
fn compute_endorsement_weight(delivery_method: &str, relationship_depth: &str) -> f32 {
    let base = match delivery_method {
        "qr" => 1.0,
        "video" => 0.7,
        "text" => 0.4,
        "email" => 0.2,
        _ => 0.2, // defensive default
    };
    let multiplier = match relationship_depth {
        "years" => 1.0,
        "months" => 0.7,
        "acquaintance" => 0.5,
        _ => 0.5,
    };
    (base * multiplier).clamp(f32::MIN_POSITIVE, 1.0)
}
```

- Include `relationship_depth` and `weight` in the INSERT statement

**Step 4: Run test to verify it passes**

Run: `cargo test --test trust_invite_tests create_invite_with_weight_fields -- --nocapture`
Expected: PASS

**Step 5: Commit**

```bash
git add service/src/trust/repo/invites.rs service/tests/trust_invite_tests.rs
git commit -m "feat(trust): store weight and relationship_depth on invites (#656)"
```

---

### Task 3: Backend — Wire Weight Through Invite Acceptance

**Files:**
- Modify: `service/src/trust/http/mod.rs` (`accept_invite_handler`, ~line 381)

**Step 1: Write the failing test**

Add to `service/tests/trust_invite_tests.rs` (or `trust_http_tests.rs`):

```rust
#[tokio::test]
async fn accept_invite_uses_stored_weight() {
    // Setup: create invite with delivery_method="text", relationship_depth="acquaintance"
    // Accept the invite
    // Verify the created endorsement has weight = 0.4 * 0.5 = 0.2
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test trust_invite_tests accept_invite_uses_stored_weight -- --nocapture`
Expected: FAIL — endorsement weight is 1.0 (hardcoded).

**Step 3: Update `accept_invite_handler`**

In `service/src/trust/http/mod.rs` around line 385, change:

```rust
// Before:
trust_service.endorse(invite.endorser_id, auth.account_id, 1.0, Some(invite.attestation.clone()))

// After:
trust_service.endorse(invite.endorser_id, auth.account_id, invite.weight, Some(invite.attestation.clone()))
```

This requires the invite struct returned from the repo to include the `weight` field. Update the `Invite` struct if needed.

**Step 4: Run test to verify it passes**

Run: `cargo test --test trust_invite_tests accept_invite_uses_stored_weight -- --nocapture`
Expected: PASS

**Step 5: Run full trust test suite**

Run: `cargo test --test trust_invite_tests --test trust_http_tests -- --nocapture`
Expected: All pass.

**Step 6: Commit**

```bash
git add service/src/trust/http/mod.rs service/tests/
git commit -m "feat(trust): use stored invite weight on acceptance instead of hardcoded 1.0 (#656)"
```

---

### Task 4: Frontend — Weight Computation Utility

**Files:**
- Create: `web/src/features/trust/utils/weightCalculator.ts`
- Create: `web/src/features/trust/utils/weightCalculator.test.ts`

**Step 1: Write the failing test**

```typescript
import { computeWeight, DELIVERY_METHODS, RELATIONSHIP_DEPTHS } from './weightCalculator';

describe('computeWeight', () => {
  it('computes QR + years as 1.0', () => {
    expect(computeWeight('qr', 'years')).toBe(1.0);
  });

  it('computes video + months as 0.49', () => {
    expect(computeWeight('video', 'months')).toBeCloseTo(0.49);
  });

  it('computes text + acquaintance as 0.2', () => {
    expect(computeWeight('text', 'acquaintance')).toBeCloseTo(0.2);
  });

  it('computes email + acquaintance as 0.1', () => {
    expect(computeWeight('email', 'acquaintance')).toBeCloseTo(0.1);
  });

  it('clamps to minimum above zero', () => {
    expect(computeWeight('email', 'acquaintance')).toBeGreaterThan(0);
  });
});
```

**Step 2: Run test to verify it fails**

Run: `cd web && yarn vitest src/features/trust/utils/weightCalculator.test.ts --run`
Expected: FAIL — module doesn't exist.

**Step 3: Write the implementation**

```typescript
export const DELIVERY_METHODS = [
  { value: 'qr', label: 'In-person (QR scan)', base: 1.0 },
  { value: 'video', label: 'Video chat', base: 0.7 },
  { value: 'text', label: 'Text / messaging', base: 0.4 },
  { value: 'email', label: 'Email', base: 0.2 },
] as const;

export const RELATIONSHIP_DEPTHS = [
  { value: 'years', label: 'Known for years', multiplier: 1.0 },
  { value: 'months', label: 'Known for months', multiplier: 0.7 },
  { value: 'acquaintance', label: 'Acquaintance', multiplier: 0.5 },
] as const;

export type DeliveryMethod = (typeof DELIVERY_METHODS)[number]['value'];
export type RelationshipDepth = (typeof RELATIONSHIP_DEPTHS)[number]['value'];

export function computeWeight(method: DeliveryMethod, depth: RelationshipDepth): number {
  const base = DELIVERY_METHODS.find((m) => m.value === method)?.base ?? 0.2;
  const multiplier = RELATIONSHIP_DEPTHS.find((d) => d.value === depth)?.multiplier ?? 0.5;
  return Math.max(Number.EPSILON, base * multiplier);
}

export function weightLabel(weight: number): string {
  if (weight >= 0.8) return 'Strong endorsement';
  if (weight >= 0.4) return 'Moderate endorsement';
  return 'Weak endorsement';
}
```

**Step 4: Run test to verify it passes**

Run: `cd web && yarn vitest src/features/trust/utils/weightCalculator.test.ts --run`
Expected: PASS

**Step 5: Commit**

```bash
git add web/src/features/trust/utils/
git commit -m "feat(trust): add weight computation utility (#656)"
```

---

### Task 5: Frontend — Weight Selection UI in GiveTab

**Files:**
- Modify: `web/src/features/endorsements/components/GiveTab.tsx`
- Modify: `web/src/features/trust/api/client.ts` (update `createInvite` to pass new fields)

**Step 1: Update `createInvite` API call**

In `web/src/features/trust/api/client.ts`, update `createInvite` to accept and pass `delivery_method` and `relationship_depth` parameters.

Also update `web/src/features/endorsements/api/client.ts` if it has its own `createInvite` (the endorsements module has overlapping API calls).

**Step 2: Add selectors to GiveTab**

In `GiveTab.tsx`, add before the QR code generation:

1. A `Select` (Mantine) for delivery method with options from `DELIVERY_METHODS`
2. A `Select` for relationship depth with options from `RELATIONSHIP_DEPTHS`
3. A weight preview showing the computed weight and its label
4. Use `computeWeight` and `weightLabel` from the utility

The QR-generation flow should only proceed after both selectors have values. Default to `delivery_method = 'qr'` since the existing flow is QR-based.

**Step 3: Wire through invite creation**

Pass `delivery_method` and `relationship_depth` to `createInvite`. The backend computes and stores the weight.

**Step 4: Verify manually**

Run: `just dev-frontend`
Navigate to the endorsement flow, verify selectors appear and weight preview updates.

**Step 5: Run frontend tests**

Run: `cd web && yarn vitest --run`
Expected: All pass (existing tests should not break; the selectors add to the form, not replace).

**Step 6: Commit**

```bash
git add web/src/features/endorsements/components/GiveTab.tsx web/src/features/trust/api/client.ts web/src/features/endorsements/api/client.ts
git commit -m "feat(trust): add weight selection UI to endorsement invite flow (#656)"
```

---

### Task 6: Lint and Final Validation

**Step 1:** Run `just lint`
**Step 2:** Run `just test`
**Step 3:** Fix any issues.
**Step 4:** Final commit if needed.

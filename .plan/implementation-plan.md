# Denouncement Edge Revocation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Wire denouncement to edge revocation — when user A denounces user B, A's endorsement of B is revoked and A's endorsement slot is freed.

**Architecture:** The denouncement flow already records denouncements and checks budget. We add a revocation side-effect in the worker (where actions are processed) and mutual exclusion checks in the service layer. The `influence_spent` column in `trust__denouncements` has a latent bug (NOT NULL but never bound in INSERT) — we fix that first.

**Tech Stack:** Rust (axum, sqlx), existing test harness, simulation mechanisms

---

### Task 1: Fix `influence_spent` Column Bug

**Files:**
- Modify: `service/src/trust/repo/denouncements.rs` (~line for `create_denouncement`)

**Step 1: Write the failing test**

Add to `service/tests/trust_denouncement_tests.rs`:

```rust
#[tokio::test]
async fn create_denouncement_succeeds() {
    // Setup: two accounts (accuser, target)
    // Call: create_denouncement(accuser_id, target_id, "test reason")
    // Assert: succeeds without DB error
    // Assert: denouncement row exists with influence_spent = 1.0
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test trust_denouncement_tests create_denouncement_succeeds -- --nocapture`
Expected: FAIL — `influence_spent` NOT NULL violation on INSERT.

**Step 3: Fix the INSERT**

In `service/src/trust/repo/denouncements.rs`, update the INSERT in `create_denouncement` to include `influence_spent` with a value of `1.0` (each denouncement costs 1 unit of influence).

**Step 4: Run test to verify it passes**

Run: `cargo test --test trust_denouncement_tests create_denouncement_succeeds -- --nocapture`
Expected: PASS

**Step 5: Commit**

```bash
git add service/src/trust/repo/denouncements.rs service/tests/trust_denouncement_tests.rs
git commit -m "fix(trust): bind influence_spent column in denouncement INSERT (#657)"
```

---

### Task 2: Edge Revocation on Denouncement

**Files:**
- Modify: `service/src/trust/worker.rs` (denouncement action processing)
- Modify: `service/src/trust/service.rs` (if denouncement goes through service layer)

**Step 1: Write the failing test**

Add to `service/tests/trust_denouncement_tests.rs`:

```rust
#[tokio::test]
async fn denouncement_revokes_endorsement_edge() {
    // Setup: A endorses B (creates endorsement edge)
    // Verify: endorsement exists
    // Action: A denounces B
    // Assert: A's endorsement of B is revoked (revoked_at is set)
    // Assert: A's endorsement slot is freed (budget shows one more available)
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test trust_denouncement_tests denouncement_revokes_endorsement_edge -- --nocapture`
Expected: FAIL — denouncement doesn't revoke the endorsement.

**Step 3: Add revocation to denouncement processing**

In `service/src/trust/worker.rs`, find where "denounce" actions are processed. After recording the denouncement, add:

```rust
// Revoke endorser's edge to target if one exists
// This uses the existing reputation_repo.revoke_endorsement which sets revoked_at
if let Err(e) = reputation_repo.revoke_endorsement(actor_id, target_id, "trust").await {
    tracing::debug!(%actor_id, %target_id, "no endorsement to revoke on denouncement: {e}");
    // Not an error — denouncement without existing endorsement is valid
}
```

The `revoke_endorsement` function already exists (used by `revoke_handler`) — it sets `revoked_at` on `reputation__endorsements`. If no endorsement exists, it's a no-op or returns an error that we can ignore.

After revocation, trigger score recomputation (same pattern as the existing revoke action handler).

**Step 4: Run test to verify it passes**

Run: `cargo test --test trust_denouncement_tests denouncement_revokes_endorsement_edge -- --nocapture`
Expected: PASS

**Step 5: Commit**

```bash
git add service/src/trust/worker.rs service/tests/trust_denouncement_tests.rs
git commit -m "feat(trust): revoke endorsement edge on denouncement (#657)"
```

---

### Task 3: Denouncement Without Existing Endorsement

**Files:**
- Test: `service/tests/trust_denouncement_tests.rs`

**Step 1: Write the test**

```rust
#[tokio::test]
async fn denouncement_without_endorsement_succeeds() {
    // Setup: A and B exist, but A has NOT endorsed B
    // Action: A denounces B
    // Assert: denouncement is recorded (budget decremented)
    // Assert: no edge changes (no endorsement existed to revoke)
    // Assert: no errors
}
```

**Step 2: Run test**

Run: `cargo test --test trust_denouncement_tests denouncement_without_endorsement_succeeds -- --nocapture`
Expected: PASS (if Task 2 handled the no-endorsement case correctly with the debug log).

If it fails, fix the error handling in the worker to not fail on missing endorsement.

**Step 3: Commit**

```bash
git add service/tests/trust_denouncement_tests.rs
git commit -m "test(trust): verify denouncement works without existing endorsement (#657)"
```

---

### Task 4: Mutual Exclusion — Cannot Endorse After Denouncing

**Files:**
- Modify: `service/src/trust/service.rs` (endorsement validation)
- Test: `service/tests/trust_denouncement_tests.rs`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn cannot_endorse_someone_you_denounced() {
    // Setup: A denounces B
    // Action: A tries to endorse B
    // Assert: endorsement is rejected with appropriate error
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test trust_denouncement_tests cannot_endorse_someone_you_denounced -- --nocapture`
Expected: FAIL — endorsement succeeds (no mutual exclusion check).

**Step 3: Add mutual exclusion check**

In `service/src/trust/service.rs`, in the `endorse` method (before enqueuing the action), add a check:

```rust
// Check if endorser has an active denouncement against the subject
let denouncement_exists = trust_repo
    .has_active_denouncement(endorser_id, subject_id)
    .await?;
if denouncement_exists {
    return Err(anyhow::anyhow!("cannot endorse a user you have denounced"));
}
```

Add `has_active_denouncement(accuser_id, target_id) -> bool` to the denouncements repo.

**Step 4: Run test to verify it passes**

Run: `cargo test --test trust_denouncement_tests cannot_endorse_someone_you_denounced -- --nocapture`
Expected: PASS

**Step 5: Write reverse test — cannot denounce someone you've endorsed (already handled)**

Actually, per the ticket behavior: denouncing someone you've endorsed IS allowed — it revokes the endorsement. So we only need the one-way check: cannot endorse after denouncing. Verify this is the case.

**Step 6: Commit**

```bash
git add service/src/trust/service.rs service/src/trust/repo/denouncements.rs service/tests/trust_denouncement_tests.rs
git commit -m "feat(trust): prevent endorsing a denounced user (#657)"
```

---

### Task 5: Simulation Harness — Denouncer Revocation Mechanism

**Files:**
- Modify: `service/tests/common/simulation/mechanisms.rs`
- Test: `service/tests/trust_simulation_tests.rs`

**Step 1: Add `apply_denouncer_revocation` function**

In `service/tests/common/simulation/mechanisms.rs`, add a new mechanism following the pattern of existing functions (`apply_edge_removal`, `apply_score_penalty`, `apply_sponsorship_cascade`):

```rust
/// Simulate denouncer-only edge revocation: denouncer's edge to target is removed.
/// Models ADR-024 behavior where denouncement revokes the denouncer's endorsement.
pub async fn apply_denouncer_revocation(
    g: &TestGraph,
    denouncer: &str,      // node name in graph
    target: &str,         // node name in graph
    anchor: &str,         // anchor node
    pool: &PgPool,
) -> SimulationReport {
    // 1. Remove edge from denouncer -> target
    // 2. Recompute scores from anchor
    // 3. Return SimulationReport with before/after scores
}
```

Follow the exact pattern from `apply_edge_removal` but scoped to a single directed edge (denouncer → target) rather than all edges to/from a node.

**Step 2: Write simulation test**

In `service/tests/trust_simulation_tests.rs`, add a test that:
- Builds a test graph
- Applies denouncer revocation
- Verifies the target's score decreases appropriately

**Step 3: Run simulation tests**

Run: `cargo test --test trust_simulation_tests -- --nocapture`
Expected: PASS

**Step 4: Commit**

```bash
git add service/tests/common/simulation/mechanisms.rs service/tests/trust_simulation_tests.rs
git commit -m "feat(trust): add denouncer revocation to simulation harness (#657)"
```

---

### Task 6: Lint and Final Validation

**Step 1:** Run `just lint`
**Step 2:** Run `just test`
**Step 3:** Fix any issues.
**Step 4:** Final commit if needed.

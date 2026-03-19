# Endorsement Split (Phase 1) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Allow endorsements beyond the slot limit (k=3). Out-of-slot endorsements are stored but don't contribute to trust graph computation. First k endorsements are auto-slotted; overflow is out-of-slot. Phase 2 (explicit slot management UX) is a separate ticket.

**Architecture:** Add `in_slot BOOLEAN` column to `reputation__endorsements`. Service layer changes hard-reject to soft-accept with `in_slot = false` when at capacity. Engine SQL filters on `in_slot = true`. Budget response includes out-of-slot count. Frontend removes hard block on full slots.

**Tech Stack:** Rust (sqlx, axum), PostgreSQL migrations, React/Mantine/TypeScript

---

### Task 1: Database migration — add `in_slot` column

**Files:**
- Create: `service/migrations/21_endorsement_in_slot.sql`

**Step 1: Write the migration**

```sql
-- Add in_slot flag to endorsements. Existing rows default to true (all current endorsements are in-slot).
-- Out-of-slot endorsements are stored but excluded from trust graph computation.
ALTER TABLE reputation__endorsements ADD COLUMN in_slot BOOLEAN NOT NULL DEFAULT true;

-- Index for efficient budget counting: only count in-slot endorsements.
CREATE INDEX idx_endorsements_in_slot_budget
    ON reputation__endorsements (endorser_id)
    WHERE topic = 'trust' AND revoked_at IS NULL AND in_slot = true;
```

**Step 2: Verify migration number is available**

Run: `ls service/migrations/*.sql | sort -V | tail -3`
Expected: Last file is `20_room_engine_columns.sql`, confirming 21 is available.

**Step 3: Commit**

```bash
git add service/migrations/21_endorsement_in_slot.sql
git commit -m "feat(trust): add in_slot column to endorsements (#754)"
```

---

### Task 2: Backend — count only in-slot endorsements for budget

**Files:**
- Modify: `service/src/reputation/repo/endorsements.rs` (lines 204-222)
- Modify: `service/src/reputation/repo/mod.rs` (lines 53-56)

**Step 1: Write the failing test**

Add to `service/tests/trust_service_tests.rs`:

```rust
#[shared_runtime_test]
async fn test_out_of_slot_endorsement_not_counted_in_budget() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let endorser = AccountFactory::new()
        .with_seed(240)
        .create(&pool)
        .await
        .expect("create endorser");

    let subject = AccountFactory::new()
        .with_seed(241)
        .create(&pool)
        .await
        .expect("create subject");

    // Insert an out-of-slot endorsement directly
    sqlx::query(
        "INSERT INTO reputation__endorsements (endorser_id, subject_id, topic, weight, in_slot) \
         VALUES ($1, $2, 'trust', 1.0, false)",
    )
    .bind(endorser.id)
    .bind(subject.id)
    .execute(&pool)
    .await
    .expect("seed out-of-slot endorsement");

    let rep_repo = PgReputationRepo::new(pool.clone());
    let count = rep_repo
        .count_active_trust_endorsements_by(endorser.id)
        .await
        .expect("count");

    // Out-of-slot endorsement should NOT be counted
    assert_eq!(count, 0, "out-of-slot endorsement should not count toward budget");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test trust_service_tests test_out_of_slot_endorsement_not_counted_in_budget -- --nocapture`
Expected: FAIL — the current query has no `in_slot` filter, so count will be 1.

**Step 3: Update the count query**

In `service/src/reputation/repo/endorsements.rs`, change the SQL in `count_active_trust_endorsements_by` (line 213):

```rust
// Before:
"SELECT COUNT(*) FROM reputation__endorsements
WHERE endorser_id = $1 AND topic = 'trust' AND revoked_at IS NULL"

// After:
"SELECT COUNT(*) FROM reputation__endorsements
WHERE endorser_id = $1 AND topic = 'trust' AND revoked_at IS NULL AND in_slot = true"
```

**Step 4: Run test to verify it passes**

Run: `cargo test --test trust_service_tests test_out_of_slot_endorsement_not_counted_in_budget -- --nocapture`
Expected: PASS

**Step 5: Commit**

```bash
git add service/src/reputation/repo/endorsements.rs service/tests/trust_service_tests.rs
git commit -m "feat(trust): count only in-slot endorsements for budget (#754)"
```

---

### Task 3: Backend — service layer allows overflow with in_slot=false

**Files:**
- Modify: `service/src/trust/service.rs` (lines 126-136)
- Modify: `service/src/trust/worker.rs` (lines 88-114)
- Modify: `service/src/reputation/repo/endorsements.rs` (function `create_endorsement`)
- Modify: `service/src/reputation/repo/mod.rs` (trait method `create_endorsement`)

**Step 1: Write the failing test**

Add to `service/tests/trust_service_tests.rs`:

```rust
#[shared_runtime_test]
async fn test_endorse_beyond_slot_limit_succeeds() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let endorser = AccountFactory::new()
        .with_seed(250)
        .create(&pool)
        .await
        .expect("create endorser");

    // Create 3 subjects and fill all k=3 slots
    let mut subjects = Vec::new();
    for seed in 251..254 {
        let s = AccountFactory::new()
            .with_seed(seed)
            .create(&pool)
            .await
            .expect("create subject");
        subjects.push(s);
    }

    for subject in &subjects {
        sqlx::query(
            "INSERT INTO reputation__endorsements (endorser_id, subject_id, topic, weight, in_slot) \
             VALUES ($1, $2, 'trust', 1.0, true)",
        )
        .bind(endorser.id)
        .bind(subject.id)
        .execute(&pool)
        .await
        .expect("seed endorsement");
    }

    // 4th endorsement should succeed (not error) but be out-of-slot
    let extra_subject = AccountFactory::new()
        .with_seed(257)
        .create(&pool)
        .await
        .expect("create extra subject");

    let rep_repo = Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let repo = Arc::new(PgTrustRepo::new(pool.clone()));
    let service = DefaultTrustService::new(repo, rep_repo);

    service
        .endorse(endorser.id, extra_subject.id, 1.0, None)
        .await
        .expect("4th endorsement should succeed as out-of-slot");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test trust_service_tests test_endorse_beyond_slot_limit_succeeds -- --nocapture`
Expected: FAIL with `EndorsementSlotsExhausted { max: 3 }`

**Step 3: Update service layer — pass in_slot flag via action payload**

In `service/src/trust/service.rs`, replace the slot check block (lines 126-136):

```rust
// Before:
if !is_verifier {
    let active_count = self
        .reputation_repo
        .count_active_trust_endorsements_by(endorser_id)
        .await?;
    if active_count >= i64::from(self.endorsement_slots) {
        return Err(TrustServiceError::EndorsementSlotsExhausted {
            max: self.endorsement_slots,
        });
    }
}

// After:
let in_slot = if is_verifier {
    true
} else {
    let active_count = self
        .reputation_repo
        .count_active_trust_endorsements_by(endorser_id)
        .await?;
    active_count < i64::from(self.endorsement_slots)
};
```

And update the payload (lines 138-142) to include `in_slot`:

```rust
let payload = json!({
    "subject_id": subject_id,
    "weight": weight,
    "attestation": attestation,
    "in_slot": in_slot,
});
```

**Step 4: Update worker — pass in_slot to create_endorsement**

In `service/src/trust/worker.rs`, update the "endorse" arm (lines 90-114):

```rust
"endorse" => {
    let subject_id = parse_uuid(&action.payload, "subject_id")?;
    #[allow(clippy::cast_possible_truncation)]
    let weight = action.payload["weight"].as_f64().unwrap_or(1.0) as f32;
    let attestation = match &action.payload["attestation"] {
        serde_json::Value::Null => None,
        v => Some(v.clone()),
    };
    let in_slot = action.payload["in_slot"].as_bool().unwrap_or(true);

    self.reputation_repo
        .create_endorsement(
            subject_id,
            "trust",
            Some(action.actor_id),
            None,
            weight,
            attestation.as_ref(),
            in_slot,
        )
        .await
        .map_err(|e| anyhow::anyhow!("create_endorsement failed: {e}"))?;

    self.trust_engine
        .recompute_from_anchor(action.actor_id, self.trust_repo.as_ref())
        .await?;
}
```

**Step 5: Update create_endorsement — accept in_slot parameter**

In `service/src/reputation/repo/mod.rs`, update the trait method signature (line 25-33):

```rust
async fn create_endorsement(
    &self,
    subject_id: Uuid,
    topic: &str,
    endorser_id: Option<Uuid>,
    evidence: Option<&serde_json::Value>,
    weight: f32,
    attestation: Option<&serde_json::Value>,
    in_slot: bool,
) -> Result<CreatedEndorsement, EndorsementRepoError>;
```

In `service/src/reputation/repo/endorsements.rs`, update the function (lines 74-114):

Add `in_slot: bool` parameter after `attestation`. Update the SQL:

```rust
let row: (Uuid,) = sqlx::query_as(
    r"
    INSERT INTO reputation__endorsements
        (id, subject_id, topic, endorser_id, evidence, weight, attestation, in_slot)
    VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
    ON CONFLICT (subject_id, topic, endorser_id)
        DO UPDATE SET weight = EXCLUDED.weight, attestation = EXCLUDED.attestation, in_slot = EXCLUDED.in_slot
    RETURNING id
    ",
)
.bind(id)
.bind(subject_id)
.bind(topic)
.bind(endorser_id)
.bind(evidence)
.bind(weight)
.bind(attestation)
.bind(in_slot)
.fetch_one(executor)
.await
.map_err(EndorsementRepoError::Database)?;
```

Also update the `PgReputationRepo` impl to pass through the new parameter.

**Step 6: Fix all other callers of create_endorsement**

Search for all callers: the demo verifier sim and any direct DB inserts. Add `true` as the `in_slot` argument to all existing callers that aren't the worker (they should all be in-slot by default).

Run: `cargo build` to find all compilation errors from the signature change and fix them.

**Step 7: Run test to verify it passes**

Run: `cargo test --test trust_service_tests test_endorse_beyond_slot_limit_succeeds -- --nocapture`
Expected: PASS

**Step 8: Run existing slot exhaustion test — it should now pass differently**

The test `test_endorsement_slots_exhausted` expects `EndorsementSlotsExhausted`. Since we no longer error, this test needs updating. Change it to verify the endorsement succeeds but is out-of-slot:

```rust
#[shared_runtime_test]
async fn test_endorsement_slots_exhausted() {
    // ... (same setup as before: 3 endorsements filling k=3 slots)

    let rep_repo = Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let repo = Arc::new(PgTrustRepo::new(pool));
    let service = DefaultTrustService::new(repo, rep_repo);

    // 4th endorsement should succeed (no longer errors)
    service
        .endorse(endorser.id, extra_subject.id, 1.0, None)
        .await
        .expect("4th endorsement should succeed as out-of-slot");
}
```

**Step 9: Run full test suite**

Run: `cargo test --test trust_service_tests -- --nocapture`
Expected: All tests pass.

**Step 10: Commit**

```bash
git add service/src/trust/service.rs service/src/trust/worker.rs \
       service/src/reputation/repo/endorsements.rs service/src/reputation/repo/mod.rs \
       service/tests/trust_service_tests.rs
git commit -m "feat(trust): allow endorsements beyond slot limit as out-of-slot (#754)"
```

---

### Task 4: Engine SQL — filter in_slot for trust graph computation

**Files:**
- Modify: `service/src/trust/engine.rs` (lines 60-96 and 150-163)

**Step 1: Add `AND in_slot = true` to distance CTE**

In `compute_distances_from`, both the base case (line 72) and recursive case (line 87) need the filter. Add after `AND e.topic = 'trust'`:

```sql
AND e.in_slot = true
```

This appears in two places in the CTE (base SELECT and recursive SELECT).

**Step 2: Add `AND in_slot = true` to diversity edge query**

In `compute_diversity_from` (line 156), add after `AND topic = 'trust'`:

```sql
AND in_slot = true
```

**Step 3: Run backend tests**

Run: `cargo test -- --nocapture`
Expected: All pass. Out-of-slot endorsements are now invisible to the trust engine.

**Step 4: Commit**

```bash
git add service/src/trust/engine.rs
git commit -m "feat(trust): engine filters on in_slot for graph computation (#754)"
```

---

### Task 5: Budget endpoint — add out_of_slot_count

**Files:**
- Modify: `service/src/trust/http/mod.rs` (lines 80-87 and 310-361)
- Modify: `service/src/reputation/repo/endorsements.rs` (add new function)
- Modify: `service/src/reputation/repo/mod.rs` (add trait method)

**Step 1: Add count_all_active_trust_endorsements_by (includes out-of-slot)**

In `service/src/reputation/repo/endorsements.rs`, add after `count_active_trust_endorsements_by`:

```rust
/// Count ALL active trust endorsements (including out-of-slot).
pub async fn count_all_active_trust_endorsements_by<'e, E>(
    executor: E,
    endorser_id: Uuid,
) -> Result<i64, EndorsementRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let count: i64 = sqlx::query_scalar(
        r"
        SELECT COUNT(*) FROM reputation__endorsements
        WHERE endorser_id = $1 AND topic = 'trust' AND revoked_at IS NULL
        ",
    )
    .bind(endorser_id)
    .fetch_one(executor)
    .await?;

    Ok(count)
}
```

Add to trait in `mod.rs` and implement in `PgReputationRepo`.

**Step 2: Update BudgetResponse**

```rust
#[derive(Debug, Serialize)]
pub struct BudgetResponse {
    pub slots_total: u32,
    pub slots_used: i64,
    pub slots_available: i64,
    pub out_of_slot_count: i64,
    pub denouncements_total: u32,
    pub denouncements_used: i64,
    pub denouncements_available: i64,
}
```

**Step 3: Update budget_handler to compute out_of_slot_count**

```rust
let all_endorsements = reputation_repo
    .count_all_active_trust_endorsements_by(auth.account_id)
    .await
    .unwrap_or(0);

// endorsements_used is already in-slot only (from count_active_trust_endorsements_by)
let out_of_slot = all_endorsements - endorsements_used;
```

Add `out_of_slot_count: out_of_slot` to the BudgetResponse construction.

**Step 4: Run backend tests and lint**

Run: `cargo test && cargo clippy -- -D warnings`
Expected: All pass.

**Step 5: Commit**

```bash
git add service/src/trust/http/mod.rs service/src/reputation/repo/endorsements.rs \
       service/src/reputation/repo/mod.rs
git commit -m "feat(trust): budget endpoint reports out-of-slot endorsement count (#754)"
```

---

### Task 6: Frontend — remove hard block, show warning instead

**Files:**
- Modify: `web/src/features/endorsements/types.ts` (line 14-21)
- Modify: `web/src/features/endorsements/components/GiveTab.tsx` (lines 67-73)
- Modify: `web/src/features/endorsements/components/SlotCounter.tsx`

**Step 1: Update BudgetResponse type**

In `web/src/features/endorsements/types.ts`, add `out_of_slot_count`:

```typescript
export interface BudgetResponse {
  slots_total: number;
  slots_used: number;
  slots_available: number;
  out_of_slot_count: number;
  denouncements_total: number;
  denouncements_used: number;
  denouncements_available: number;
}
```

**Step 2: Replace hard block with info alert**

In `web/src/features/endorsements/components/GiveTab.tsx`, replace lines 67-73:

```tsx
// Before:
if (slotsAvailable <= 0) {
  return (
    <Alert color="yellow" title="No slots available">
      All endorsement slots used. Revoke an existing endorsement to endorse someone new.
    </Alert>
  );
}

// After (remove the early return entirely — show warning inline instead):
```

Add a warning alert INSIDE the return JSX, before the form fields (after line 76, inside the `<Stack>`):

```tsx
{slotsAvailable <= 0 && (
  <Alert color="blue" title="Out-of-slot endorsement">
    All trust graph slots are used. This endorsement will be stored but won't
    contribute to trust scores. It can still grant room access.
  </Alert>
)}
```

**Step 3: Update SlotCounter to show out-of-slot count**

In `web/src/features/endorsements/components/SlotCounter.tsx`, the `SlotCounterProps` interface should accept an optional `outOfSlot` prop. Update the display to show "3 of 3 in-slot + 2 additional" or similar. This is a cosmetic change — exact wording TBD.

**Step 4: Run frontend lint and tests**

Run: `cd web && yarn lint && yarn vitest --run`
Expected: All pass.

**Step 5: Commit**

```bash
git add web/src/features/endorsements/
git commit -m "feat(trust): frontend allows out-of-slot endorsements with warning (#754)"
```

---

### Task 7: Verify full stack

**Step 1: Run full lint**

Run: `just lint`

**Step 2: Run full test suite**

Run: `just test`

**Step 3: Run codegen (schema may have changed)**

Run: `just codegen`
If files changed, commit them.

**Step 4: Final commit if needed**

```bash
git add -A && git commit -m "chore: codegen after endorsement split (#754)"
```

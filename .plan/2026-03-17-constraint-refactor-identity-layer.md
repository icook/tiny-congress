# Constraint Refactor & Identity Verification Layer — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Refactor the room constraint system so constraints are self-contained policy objects constructed from configuration, then add an `identity_verified` constraint type that checks Layer 1 attestation directly — unblocking #665 (verified users can't vote).

**Architecture:** The current constraint system takes `room_anchor_id: Option<Uuid>` as an external parameter, always passes `None`, and all three constraint types route through `trust__score_snapshots`. This refactor internalizes configuration into each constraint at construction time (via `constraint_config` JSONB), removes the anchor parameter from the trait, and adds a new `identity_verified` constraint that queries `reputation__endorsements` directly — no trust graph traversal needed. This implements the two-layer separation from ADR-017: Layer 1 (identity attestation) and Layer 2 (trust graph reachability) become two families of constraints behind the same `RoomConstraint` trait.

**Tech Stack:** Rust, async-trait, sqlx (Postgres), serde_json. Tests use testcontainers.

**Key design references:**
- ADR-017: Two-Layer Trust Architecture (Layer 1 = identity, Layer 2 = trust graph)
- `.plan/2026-03-17-room-types-architecture.md`: Container/module separation — constraints are container-level, self-configured
- Open question Q31: Room types architecture

---

## Task 1: Add `has_identity_endorsement` to TrustRepo

The `identity_verified` constraint needs to query `reputation__endorsements` directly. No existing `TrustRepo` method does this — the endorsements table is only accessed by the trust engine internally.

**Files:**
- Modify: `service/src/trust/repo/mod.rs:104-205` (TrustRepo trait)
- Modify: `service/src/trust/repo/endorsements.rs` (or create if endorsement queries don't exist here)
- Test: `service/tests/trust_constraint_tests.rs`

**Step 1: Write the failing test**

Add to `service/tests/trust_constraint_tests.rs`:

```rust
#[sqlx::test]
async fn test_has_identity_endorsement(pool: PgPool) {
    let repo = PgTrustRepo::new(pool.clone());
    let verifier_id = seed_account(&pool, "verifier").await;
    let user_id = seed_account(&pool, "user").await;
    let other_id = seed_account(&pool, "other").await;

    // No endorsement yet — should return false
    let result = repo
        .has_identity_endorsement(user_id, &[verifier_id], "identity_verified")
        .await
        .unwrap();
    assert!(!result);

    // Insert endorsement
    sqlx::query(
        "INSERT INTO reputation__endorsements (endorser_id, endorsee_id, topic, evidence)
         VALUES ($1, $2, $3, '{}'::jsonb)"
    )
    .bind(verifier_id)
    .bind(user_id)
    .bind("identity_verified")
    .execute(&pool)
    .await
    .unwrap();

    // Now should return true
    let result = repo
        .has_identity_endorsement(user_id, &[verifier_id], "identity_verified")
        .await
        .unwrap();
    assert!(result);

    // Different verifier — should return false
    let result = repo
        .has_identity_endorsement(user_id, &[other_id], "identity_verified")
        .await
        .unwrap();
    assert!(!result);

    // User not endorsed — should return false
    let result = repo
        .has_identity_endorsement(other_id, &[verifier_id], "identity_verified")
        .await
        .unwrap();
    assert!(!result);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test trust_constraint_tests test_has_identity_endorsement -- --nocapture`
Expected: FAIL — `has_identity_endorsement` method does not exist.

**Step 3: Add trait method and implementation**

In `service/src/trust/repo/mod.rs`, add to the `TrustRepo` trait (after `get_all_scores` ~line 204):

```rust
    async fn has_identity_endorsement(
        &self,
        user_id: Uuid,
        verifier_ids: &[Uuid],
        topic: &str,
    ) -> Result<bool, TrustRepoError>;
```

In the `PgTrustRepo` impl block, add:

```rust
    async fn has_identity_endorsement(
        &self,
        user_id: Uuid,
        verifier_ids: &[Uuid],
        topic: &str,
    ) -> Result<bool, TrustRepoError> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1 FROM reputation__endorsements
                WHERE endorsee_id = $1
                  AND endorser_id = ANY($2)
                  AND topic = $3
            )"
        )
        .bind(user_id)
        .bind(verifier_ids)
        .bind(topic)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| TrustRepoError::Database(e.to_string()))?;

        Ok(exists)
    }
```

**Step 4: Run test to verify it passes**

Run: `cargo test --test trust_constraint_tests test_has_identity_endorsement -- --nocapture`
Expected: PASS

**Step 5: Commit**

```bash
git add service/src/trust/repo/
git commit -m "feat(trust): add has_identity_endorsement to TrustRepo

Layer 1 identity checks need to query reputation__endorsements
directly, bypassing the trust graph. This is the first TrustRepo
method that reads endorsements outside the engine.

Part of #665"
```

---

## Task 2: Create `IdentityVerifiedConstraint`

New constraint type that checks Layer 1 attestation. No anchor, no trust graph traversal — just "has a recognized verifier attested this user?"

**Files:**
- Modify: `service/src/trust/constraints.rs:1-297`
- Test: `service/tests/trust_constraint_tests.rs`

**Step 1: Write the failing test**

Add to `service/tests/trust_constraint_tests.rs`:

```rust
#[sqlx::test]
async fn test_identity_verified_eligible(pool: PgPool) {
    let repo = PgTrustRepo::new(pool.clone());
    let verifier_id = seed_account(&pool, "verifier").await;
    let user_id = seed_account(&pool, "user").await;

    // Seed endorsement
    sqlx::query(
        "INSERT INTO reputation__endorsements (endorser_id, endorsee_id, topic, evidence)
         VALUES ($1, $2, 'identity_verified', '{}'::jsonb)"
    )
    .bind(verifier_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();

    let constraint = IdentityVerifiedConstraint::new(vec![verifier_id]);
    let result = constraint.check(user_id, &repo).await.unwrap();
    assert!(result.is_eligible);
}

#[sqlx::test]
async fn test_identity_verified_ineligible(pool: PgPool) {
    let repo = PgTrustRepo::new(pool.clone());
    let verifier_id = seed_account(&pool, "verifier").await;
    let user_id = seed_account(&pool, "user").await;

    // No endorsement
    let constraint = IdentityVerifiedConstraint::new(vec![verifier_id]);
    let result = constraint.check(user_id, &repo).await.unwrap();
    assert!(!result.is_eligible);
    assert!(result.reason.unwrap().contains("identity verification"));
}
```

Note: This test uses the NEW trait signature `check(user_id, trust_repo)` without `room_anchor_id`. This will fail until Task 3 changes the trait. **Write the test now but don't run it yet — it compiles against the new signature that Task 3 introduces.** Alternatively, write it with the old signature and update in Task 3.

**Step 2: Write the constraint**

Add to `service/src/trust/constraints.rs` (after `CongressConstraint` impl, before `build_constraint`):

```rust
/// Layer 1 constraint: checks identity attestation from recognized verifiers.
/// Does not use the trust graph — queries reputation__endorsements directly.
pub struct IdentityVerifiedConstraint {
    verifier_ids: Vec<Uuid>,
}

impl IdentityVerifiedConstraint {
    pub fn new(verifier_ids: Vec<Uuid>) -> Self {
        Self { verifier_ids }
    }
}

#[async_trait]
impl RoomConstraint for IdentityVerifiedConstraint {
    async fn check(
        &self,
        user_id: Uuid,
        room_anchor_id: Option<Uuid>,  // ignored — Layer 1 doesn't use anchors
        trust_repo: &dyn TrustRepo,
    ) -> Result<Eligibility, anyhow::Error> {
        let verified = trust_repo
            .has_identity_endorsement(user_id, &self.verifier_ids, "identity_verified")
            .await?;

        if verified {
            Ok(Eligibility {
                is_eligible: true,
                reason: None,
            })
        } else {
            Ok(Eligibility {
                is_eligible: false,
                reason: Some(
                    "User has not completed identity verification from a recognized verifier"
                        .to_string(),
                ),
            })
        }
    }
}
```

**Step 3: Add to `build_constraint` factory**

In `service/src/trust/constraints.rs`, add a new arm to `build_constraint` (~line 280):

```rust
"identity_verified" => {
    let verifier_ids = config
        .get("verifier_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok()))
                .collect::<Vec<_>>()
        })
        .ok_or_else(|| anyhow::anyhow!(
            "identity_verified constraint requires verifier_ids array in config"
        ))?;
    if verifier_ids.is_empty() {
        anyhow::bail!("identity_verified constraint requires at least one verifier_id");
    }
    Ok(Box::new(IdentityVerifiedConstraint::new(verifier_ids)))
}
```

**Step 4: Add factory test**

Add to the existing `test_build_constraint_factory` test in `trust_constraint_tests.rs`:

```rust
// identity_verified
let verifier_id = Uuid::new_v4();
let config = serde_json::json!({"verifier_ids": [verifier_id.to_string()]});
let constraint = build_constraint("identity_verified", &config);
assert!(constraint.is_ok());

// identity_verified — missing verifier_ids
let config = serde_json::json!({});
let constraint = build_constraint("identity_verified", &config);
assert!(constraint.is_err());

// identity_verified — empty verifier_ids
let config = serde_json::json!({"verifier_ids": []});
let constraint = build_constraint("identity_verified", &config);
assert!(constraint.is_err());
```

**Step 5: Run tests**

Run: `cargo test --test trust_constraint_tests -- --nocapture`
Expected: All existing tests PASS + new tests PASS. The `IdentityVerifiedConstraint` uses the current trait signature (with `room_anchor_id`) — it just ignores the parameter.

**Step 6: Commit**

```bash
git add service/src/trust/constraints.rs service/tests/trust_constraint_tests.rs
git commit -m "feat(trust): add identity_verified constraint type (Layer 1)

New constraint checks reputation__endorsements directly for identity
attestation from recognized verifiers. No trust graph traversal,
no anchor needed. Implements ADR-017 Layer 1 / Layer 2 separation
in the constraint system.

Part of #665"
```

---

## Task 3: Internalize anchor into trust-graph constraints

Refactor `EndorsedByConstraint`, `CommunityConstraint`, and `CongressConstraint` to read `anchor_id` from their config at construction time. Change the `RoomConstraint` trait to drop the `room_anchor_id` parameter.

**Files:**
- Modify: `service/src/trust/constraints.rs`
- Modify: `service/src/rooms/service.rs:538-544` (cast_vote)
- Modify: `service/tests/trust_constraint_tests.rs` (update all existing tests)
- Modify: `service/tests/rooms_handler_tests.rs:66-77` (update `make_eligible` helper)
- Modify: `service/tests/trust_e2e_tests.rs:122` (update constraint check call)

**Step 1: Change the trait signature**

In `service/src/trust/constraints.rs`, update `RoomConstraint`:

```rust
#[async_trait]
pub trait RoomConstraint: Send + Sync {
    async fn check(
        &self,
        user_id: Uuid,
        trust_repo: &dyn TrustRepo,
    ) -> Result<Eligibility, anyhow::Error>;
}
```

**Step 2: Update `EndorsedByConstraint`**

```rust
pub struct EndorsedByConstraint {
    anchor_id: Uuid,
}

impl EndorsedByConstraint {
    pub fn new(anchor_id: Uuid) -> Self {
        Self { anchor_id }
    }
}

#[async_trait]
impl RoomConstraint for EndorsedByConstraint {
    async fn check(
        &self,
        user_id: Uuid,
        trust_repo: &dyn TrustRepo,
    ) -> Result<Eligibility, anyhow::Error> {
        let snapshot = trust_repo
            .get_score(user_id, Some(self.anchor_id))
            .await?;

        match snapshot {
            Some(s) if s.trust_distance.is_some() => Ok(Eligibility {
                is_eligible: true,
                reason: None,
            }),
            _ => Ok(Eligibility {
                is_eligible: false,
                reason: Some(format!(
                    "User is not reachable in the trust graph from anchor {}",
                    self.anchor_id
                )),
            }),
        }
    }
}
```

**Step 3: Update `CommunityConstraint` and `CongressConstraint`**

Add `anchor_id: Uuid` field to both structs. Update constructors:

```rust
pub struct CommunityConstraint {
    anchor_id: Uuid,
    max_distance: f32,
    min_diversity: i32,
}

impl CommunityConstraint {
    pub fn new(anchor_id: Uuid, max_distance: f32, min_diversity: i32) -> Result<Self, anyhow::Error> {
        // existing validation...
        Ok(Self { anchor_id, max_distance, min_diversity })
    }
}
```

Same pattern for `CongressConstraint`. Both `check()` methods use `Some(self.anchor_id)` instead of the parameter.

**Step 4: Update `build_constraint` factory**

All three trust-graph constraint types now parse `anchor_id` from config:

```rust
"endorsed_by" => {
    let anchor_id = parse_uuid_from_config(config, "anchor_id")?;
    Ok(Box::new(EndorsedByConstraint::new(anchor_id)))
}
"community" => {
    let anchor_id = parse_uuid_from_config(config, "anchor_id")?;
    let max_distance = get_f64_or_default(config, "max_distance", 5.0) as f32;
    let min_diversity = get_i64_or_default(config, "min_diversity", 2) as i32;
    Ok(Box::new(CommunityConstraint::new(anchor_id, max_distance, min_diversity)?))
}
"congress" => {
    let anchor_id = parse_uuid_from_config(config, "anchor_id")?;
    let min_diversity = get_i64_or_default(config, "min_diversity", 3) as i32;
    Ok(Box::new(CongressConstraint::new(anchor_id, min_diversity)?))
}
```

Add helper:

```rust
fn parse_uuid_from_config(config: &serde_json::Value, key: &str) -> Result<Uuid, anyhow::Error> {
    config
        .get(key)
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| anyhow::anyhow!("constraint config requires valid UUID for '{key}'"))
}
```

**Step 5: Update `cast_vote` in `service/src/rooms/service.rs`**

Lines 536-544 simplify — remove the comment about anchor and drop `None`:

```rust
let eligibility = constraint
    .check(user_id, self.trust_repo.as_ref())
    .await
    .map_err(|e| {
        tracing::error!("Eligibility check failed: {e}");
        VoteError::Internal("Internal server error".to_string())
    })?;
```

**Step 6: Update `IdentityVerifiedConstraint` from Task 2**

Update its `check()` signature to match the new trait (drop the `room_anchor_id` parameter it was ignoring).

**Step 7: Update ALL existing tests**

Every test that calls `constraint.check(user_id, Some(anchor), &repo)` or `constraint.check(user_id, None, &repo)` must change to `constraint.check(user_id, &repo)`. The anchor is now in the constraint config.

In `trust_constraint_tests.rs`: update `test_endorsed_by_eligible`, `test_endorsed_by_ineligible`, `test_community_*`, `test_congress_*`, and `test_build_constraint_factory`. Constraint construction changes from `EndorsedByConstraint` (unit struct) to `EndorsedByConstraint::new(anchor_id)`.

In `rooms_handler_tests.rs`: the `make_eligible` helper (~line 66) currently seeds `upsert_score(account_id, None, ...)`. It needs to seed with the anchor used by the room's constraint config. This means the room's constraint_config must include a known anchor_id, and the helper must use `Some(anchor_id)` when upserting the score. Check what rooms are created in these tests and how their constraint_config is set.

In `trust_e2e_tests.rs`: update the `check()` call at line 122.

**Step 8: Run full test suite**

Run: `cargo test -- --nocapture`
Expected: ALL PASS

**Step 9: Commit**

```bash
git add service/src/trust/constraints.rs service/src/rooms/service.rs service/tests/
git commit -m "refactor(trust): internalize anchor into constraint config

Constraints are now self-contained policy objects. EndorsedByConstraint,
CommunityConstraint, and CongressConstraint read anchor_id from their
constraint_config JSONB at construction time. The RoomConstraint trait
no longer takes room_anchor_id as a parameter.

cast_vote no longer needs to know about anchors — it builds the
constraint from config and calls check(user_id, trust_repo).

Part of #665"
```

---

## Task 4: Update `create_room` to accept constraint parameters

Currently `create_room` hardcodes `constraint_type = 'endorsed_by'` and derives `constraint_config` from `eligibility_topic`. It needs to accept arbitrary constraint configuration.

**Files:**
- Modify: `service/src/rooms/repo/rooms.rs:62-86` (create_room signature + SQL)
- Modify: `service/src/rooms/service.rs` (RoomsService::create_room)
- Modify: `service/src/rooms/http/mod.rs` (create_room handler — check if it exposes constraint params)
- Test: `service/tests/rooms_handler_tests.rs`

**Step 1: Update `create_room` repo function**

Change signature to accept `constraint_type` and `constraint_config`:

```rust
pub async fn create_room<'e, E>(
    executor: E,
    name: &str,
    description: Option<&str>,
    eligibility_topic: &str,  // keep for backward compat, can be deprecated later
    poll_duration_secs: Option<i32>,
    constraint_type: &str,
    constraint_config: &serde_json::Value,
) -> Result<RoomRecord, RoomRepoError>
```

Update the INSERT SQL to use `$6` and `$7` instead of hardcoded `'endorsed_by'` and `jsonb_build_object('topic', $3)`.

**Step 2: Update callers**

Find all callers of `create_room` in the service layer and tests. Update them to pass the constraint parameters explicitly. Existing callers that used the implicit `endorsed_by` behavior should now pass `"endorsed_by"` and a config with `anchor_id`.

**Step 3: Run tests**

Run: `cargo test-- --nocapture`
Expected: ALL PASS

**Step 4: Commit**

```bash
git add service/src/rooms/
git commit -m "refactor(rooms): accept constraint_type and config in create_room

create_room no longer hardcodes endorsed_by. Callers specify the
constraint type and its configuration explicitly. This enables
creating rooms with identity_verified or other constraint types.

Part of #665"
```

---

## Task 5: Update demo room seeding for identity_verified

Demo rooms should use `constraint_type = 'identity_verified'` with the demo verifier's account ID. The demo verifier already writes `topic = 'identity_verified'` endorsements — no changes needed to the verifier itself.

**Files:**
- Modify: wherever demo rooms are seeded (check `service/src/bin/` or seed scripts)
- Modify: `service/tests/rooms_handler_tests.rs` (update test rooms if needed)

**Step 1: Find and update demo room creation**

Search for where demo rooms are created. Update `constraint_type` to `"identity_verified"` and `constraint_config` to `{"verifier_ids": ["<demo_verifier_account_uuid>"]}`.

The demo verifier's account UUID is deterministic (derived from `SimAccount::demo_verifier()`). Find the exact UUID or derive it at room-creation time.

**Step 2: Verify end-to-end**

If a local dev environment is available, run the demo verifier + create a room + vote flow to confirm the full path works:
1. Demo verifier endorses a user with `topic = 'identity_verified'`
2. Room has `constraint_type = 'identity_verified'`, config has the verifier's account ID
3. `cast_vote` → `build_constraint` → `IdentityVerifiedConstraint` → `has_identity_endorsement` → PASS

**Step 3: Commit**

```bash
git add service/
git commit -m "feat(demo): use identity_verified constraint for demo rooms

Demo rooms now use Layer 1 identity verification instead of
trust graph reachability. The demo verifier already writes
identity_verified endorsements — rooms just needed to check
for them directly.

Fixes #665"
```

---

## Task 6: Lint and final verification

**Step 1: Run linting**

Run: `just lint`
Run: `just lint-static`

**Step 2: Run full test suite**

Run: `just test`

**Step 3: Fix any issues and commit**

---

## Summary of changes by file

| File | Change |
|------|--------|
| `service/src/trust/repo/mod.rs` | Add `has_identity_endorsement` to TrustRepo trait + PgTrustRepo impl |
| `service/src/trust/constraints.rs` | New `IdentityVerifiedConstraint`; internalize anchor into existing constraints; simplify trait signature; update factory |
| `service/src/rooms/service.rs` | Simplify `cast_vote` — drop anchor param from constraint.check() |
| `service/src/rooms/repo/rooms.rs` | Accept `constraint_type` + `constraint_config` in `create_room` |
| `service/tests/trust_constraint_tests.rs` | New tests for identity_verified; update all existing tests for new trait signature |
| `service/tests/rooms_handler_tests.rs` | Update `make_eligible` and room creation for new constraint config |
| `service/tests/trust_e2e_tests.rs` | Update constraint check call |
| Demo room seeding | Switch to `identity_verified` constraint type |

**No migrations required.** The `constraint_type` and `constraint_config` columns already exist in `rooms__rooms`. Only the Rust code that reads/writes them changes.

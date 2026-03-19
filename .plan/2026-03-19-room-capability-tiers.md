# Room Capability Tiers (Owner + Participant) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add Owner + Participant + elevated role tiers to rooms. Room creator is auto-assigned as owner. Base participant access is gated on `endorsed_by_user(owner)` — one endorsement grants access to all owner's rooms. Owner can assign elevated roles per-room for differentiation.

**Architecture:** Two-layer access model. Layer 1 (platform): `endorsed_by_user` constraint checks endorsement existence (including out-of-slot) — one endorsement, all rooms. Layer 2 (room): `rooms__role_assignments` table for per-room role elevation. New `GET /rooms/:id/my-capabilities` pre-checks eligibility. Resolution order: owner_id check → role assignment → constraint → none. Frontend replaces hardcoded `isVerified` gate with server-driven eligibility.

**Tech Stack:** Rust (sqlx, axum, tc-engine-api), PostgreSQL migration, React/Mantine/TypeScript

---

### Task 1: Migration — add `owner_id` to rooms

**Files:**
- Create: `service/migrations/22_room_owner.sql`

**Step 1: Write the migration**

```sql
-- Room owner: the account that created the room. Used for capability tier gates.
-- Existing rooms get NULL (no owner), new rooms require it.
ALTER TABLE rooms__rooms ADD COLUMN owner_id UUID REFERENCES accounts(id);

-- Per-room role assignments for elevated access (beyond base participant).
-- The endorsement gets users in the door; role assignment differentiates them.
CREATE TABLE rooms__role_assignments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    room_id UUID NOT NULL REFERENCES rooms__rooms(id),
    account_id UUID NOT NULL REFERENCES accounts(id),
    role TEXT NOT NULL,          -- e.g. "contributor", "moderator"
    assigned_by UUID NOT NULL REFERENCES accounts(id),
    assigned_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(room_id, account_id)  -- one role per user per room
);
```

Note: We allow NULL owner_id for existing rows. New room creation will always set it. The role_assignments UNIQUE constraint means reassigning a role requires an upsert.

**Step 2: Verify migration number is available**

Run: `ls service/migrations/*.sql | sort -V | tail -3`
Expected: Last file is `21_endorsement_in_slot.sql` (from #754), confirming 22 is next.

**Step 3: Commit**

```bash
git add service/migrations/22_room_owner_and_roles.sql
git commit -m "feat(rooms): add owner_id and role_assignments table (#755)"
```

---

### Task 2: Backend — update RoomRecord and create_room to set owner_id

**Files:**
- Modify: `service/src/rooms/repo/rooms.rs` — add `owner_id` to `RoomRecord`, `RoomRow`, and `create_room` SQL
- Modify: `service/src/rooms/repo/mod.rs` — update `RoomsRepo` trait + `PgRoomsRepo` impl
- Modify: `service/src/rooms/service.rs` — update `RoomsService` trait + `DefaultRoomsService` to accept `owner_id`
- Modify: `service/src/rooms/http/platform.rs` — pass `auth.account_id` as `owner_id` to `create_room`

**Step 1: Add `owner_id` to `RoomRecord` and `RoomRow`**

In `service/src/rooms/repo/rooms.rs`, add to both structs:
```rust
pub owner_id: Option<Uuid>,
```

Update `row_to_record` to pass through `owner_id`.

**Step 2: Update `create_room` SQL to INSERT `owner_id`**

Add `owner_id` parameter (type `Option<Uuid>`) to the function. Update the INSERT:
```sql
INSERT INTO rooms__rooms (name, description, eligibility_topic, poll_duration_secs, constraint_type, constraint_config, engine_type, engine_config, owner_id)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
RETURNING ...
```

Add `owner_id` to all SELECT column lists that construct `RoomRow`.

**Step 3: Update `RoomsRepo` trait and `PgRoomsRepo`**

Add `owner_id: Option<Uuid>` parameter to `create_room` in the trait and impl.

**Step 4: Update `RoomsService` trait and `DefaultRoomsService`**

Add `owner_id: Option<Uuid>` parameter to `create_room`. Pass through to repo.

**Step 5: Update `create_room` HTTP handler**

In `service/src/rooms/http/platform.rs`, pass `Some(auth.account_id)` as `owner_id`:
```rust
let room = match service
    .create_room(
        &req.name,
        req.description.as_deref(),
        &req.eligibility_topic,
        req.poll_duration_secs,
        &req.constraint_type,
        &req.constraint_config,
        Some(auth.account_id),
    )
    .await
```

**Step 6: Update `RoomResponse` to include `owner_id` and `constraint_type`**

In `service/src/rooms/http/mod.rs`:
```rust
#[derive(Debug, serde::Serialize)]
pub struct RoomResponse {
    pub id: uuid::Uuid,
    pub name: String,
    pub description: Option<String>,
    pub eligibility_topic: String,
    pub status: String,
    pub poll_duration_secs: Option<i32>,
    pub created_at: String,
    pub engine_type: String,
    pub engine_config: serde_json::Value,
    pub owner_id: Option<uuid::Uuid>,
    pub constraint_type: String,
}
```

Update `room_to_response` in `platform.rs` to include the new fields.

**Step 7: Run tests**

Run: `cargo test -- --nocapture`
Fix any compilation errors from the signature changes. All existing tests must still pass.

**Step 8: Commit**

```bash
git add service/src/rooms/
git commit -m "feat(rooms): set owner_id on room creation, include in response (#755)"
```

---

### Task 3: Backend — new `endorsed_by_user` constraint type

**Files:**
- Modify: `crates/tc-engine-api/src/constraints.rs` — add new constraint type to factory

**Step 1: Write a failing test**

Add to the tests module in `crates/tc-engine-api/src/constraints.rs`:

```rust
#[tokio::test]
async fn endorsed_by_user_eligible_when_endorsement_exists() {
    let user = Uuid::new_v4();
    let owner = Uuid::new_v4();
    let reader = MockTrustReader::new()
        .with_endorsement(user, "trust", owner);

    let config = serde_json::json!({ "endorser_id": owner.to_string() });
    let constraint = build_constraint("endorsed_by_user", &config).unwrap();
    let result = constraint.check(user, &reader).await.unwrap();
    assert!(result.is_eligible);
}

#[tokio::test]
async fn endorsed_by_user_ineligible_without_endorsement() {
    let user = Uuid::new_v4();
    let owner = Uuid::new_v4();
    let reader = MockTrustReader::new();

    let config = serde_json::json!({ "endorser_id": owner.to_string() });
    let constraint = build_constraint("endorsed_by_user", &config).unwrap();
    let result = constraint.check(user, &reader).await.unwrap();
    assert!(!result.is_eligible);
    assert!(result.reason.unwrap().contains("endorsement"));
}

#[tokio::test]
async fn endorsed_by_user_owner_is_always_eligible() {
    let owner = Uuid::new_v4();
    let reader = MockTrustReader::new();

    let config = serde_json::json!({ "endorser_id": owner.to_string() });
    let constraint = build_constraint("endorsed_by_user", &config).unwrap();
    // The owner themselves should always pass
    let result = constraint.check(owner, &reader).await.unwrap();
    assert!(result.is_eligible);
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p tc-engine-api -- endorsed_by_user --nocapture`
Expected: FAIL — `unknown constraint type: endorsed_by_user`

**Step 3: Implement `EndorsedByUserConstraint`**

Add to `crates/tc-engine-api/src/constraints.rs`:

```rust
/// User must have a trust endorsement from a specific account (the room owner).
/// Includes out-of-slot endorsements. The endorser (owner) always passes.
pub struct EndorsedByUserConstraint {
    endorser_id: Uuid,
}

impl EndorsedByUserConstraint {
    #[must_use]
    pub const fn new(endorser_id: Uuid) -> Self {
        Self { endorser_id }
    }
}

#[async_trait]
impl RoomConstraint for EndorsedByUserConstraint {
    async fn check(
        &self,
        user_id: Uuid,
        trust_reader: &dyn TrustGraphReader,
    ) -> Result<Eligibility, anyhow::Error> {
        // The endorser (room owner) is always eligible
        if user_id == self.endorser_id {
            return Ok(Eligibility {
                is_eligible: true,
                reason: None,
            });
        }

        let has = trust_reader
            .has_endorsement(user_id, "trust", &[self.endorser_id])
            .await
            .map_err(|e| anyhow::anyhow!("trust reader error: {e}"))?;

        if has {
            Ok(Eligibility {
                is_eligible: true,
                reason: None,
            })
        } else {
            Ok(Eligibility {
                is_eligible: false,
                reason: Some(format!(
                    "requires endorsement from room owner {}",
                    self.endorser_id
                )),
            })
        }
    }
}
```

**Step 4: Add to `build_constraint` factory**

Add a new arm in the match:
```rust
"endorsed_by_user" => {
    let endorser_id = parse_uuid_from_config(config, "endorser_id")?;
    Ok(Box::new(EndorsedByUserConstraint::new(endorser_id)))
}
```

**Step 5: Run tests to verify they pass**

Run: `cargo test -p tc-engine-api -- --nocapture`
Expected: All pass including the 3 new tests.

**Step 6: Commit**

```bash
git add crates/tc-engine-api/src/constraints.rs
git commit -m "feat(rooms): add endorsed_by_user constraint type (#755)"
```

---

### Task 4: Backend — auto-configure endorsed_by_user on room creation

**Files:**
- Modify: `service/src/rooms/http/platform.rs` — auto-set constraint when owner creates a room

**Step 1: Update `create_room` handler**

When a room is created with the default constraint, auto-configure it to use the creator as the endorser. In `platform.rs`, after extracting the request but before calling `service.create_room`:

```rust
// Auto-configure endorsed_by_user constraint with creator as endorser
let constraint_type = if req.constraint_type == "identity_verified" && req.constraint_config == serde_json::json!({}) {
    // Default constraint — use endorsed_by_user with creator as endorser
    "endorsed_by_user".to_string()
} else {
    req.constraint_type.clone()
};

let constraint_config = if constraint_type == "endorsed_by_user" && req.constraint_config == serde_json::json!({}) {
    serde_json::json!({ "endorser_id": auth.account_id.to_string() })
} else {
    req.constraint_config.clone()
};
```

Then pass `&constraint_type` and `&constraint_config` to `service.create_room` instead of `&req.constraint_type` and `&req.constraint_config`.

**Step 2: Update `default_constraint_type` function**

Change the default from `"identity_verified"` to `"endorsed_by_user"`:

```rust
fn default_constraint_type() -> String {
    "endorsed_by_user".to_string()
}
```

**Step 3: Run tests**

Run: `cargo test -- --nocapture`
Expected: All pass. Existing room creation tests may need updating if they assert on constraint_type.

**Step 4: Commit**

```bash
git add service/src/rooms/http/
git commit -m "feat(rooms): auto-configure endorsed_by_user constraint on room creation (#755)"
```

---

### Task 5: Backend — eligibility pre-check endpoint

**Files:**
- Modify: `service/src/rooms/http/platform.rs` — add `my_capabilities` handler
- Modify: `service/src/rooms/http/mod.rs` — add response type and route

**Step 1: Add response type**

In `service/src/rooms/http/mod.rs`:

```rust
#[derive(Debug, serde::Serialize)]
pub struct MyCapabilitiesResponse {
    pub role: String,         // "owner", "participant", "none"
    pub can_vote: bool,
    pub can_configure: bool,
    pub reason: Option<String>, // why not eligible, if role == "none"
    pub next_step: Option<String>, // what to do to become eligible
}
```

**Step 2: Add route**

In the router function:
```rust
.route("/rooms/{room_id}/my-capabilities", get(platform::my_capabilities))
```

**Step 3: Implement handler**

In `service/src/rooms/http/platform.rs`:

```rust
pub async fn my_capabilities(
    Extension(service): Extension<Arc<dyn RoomsService>>,
    Extension(trust_repo): Extension<Arc<dyn TrustRepo>>,
    Extension(pool): Extension<PgPool>,
    Path(room_id): Path<Uuid>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let room = match service.get_room(room_id).await {
        Ok(r) => r,
        Err(e) => return room_error_response(e),
    };

    // Owner check
    if room.owner_id == Some(auth.account_id) {
        return (StatusCode::OK, Json(MyCapabilitiesResponse {
            role: "owner".to_string(),
            can_vote: true,
            can_configure: true,
            reason: None,
            next_step: None,
        })).into_response();
    }

    // Check for explicit role assignment (layer 2: per-room elevation)
    let assigned_role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM rooms__role_assignments WHERE room_id = $1 AND account_id = $2"
    )
    .bind(room_id)
    .bind(auth.account_id)
    .fetch_optional(&*pool)
    .await
    .unwrap_or(None);

    if let Some(role) = assigned_role {
        return (StatusCode::OK, Json(MyCapabilitiesResponse {
            role,
            can_vote: true,
            can_configure: false,
            reason: None,
            next_step: None,
        })).into_response();
    }

    // Participant check: evaluate room constraint (layer 1: platform endorsement)
    let trust_reader = TrustRepoGraphReader::new(trust_repo);
    let constraint = match build_constraint(&room.constraint_type, &room.constraint_config) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(room_id = %room_id, "failed to build constraint: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse {
                error: "Failed to evaluate eligibility".to_string(),
            })).into_response();
        }
    };

    match constraint.check(auth.account_id, &trust_reader).await {
        Ok(eligibility) if eligibility.is_eligible => {
            (StatusCode::OK, Json(MyCapabilitiesResponse {
                role: "participant".to_string(),
                can_vote: true,
                can_configure: false,
                reason: None,
                next_step: None,
            })).into_response()
        }
        Ok(eligibility) => {
            let next_step = if room.constraint_type == "endorsed_by_user" {
                Some("Ask the room owner to endorse you".to_string())
            } else {
                Some("Complete identity verification or get endorsed".to_string())
            };
            (StatusCode::OK, Json(MyCapabilitiesResponse {
                role: "none".to_string(),
                can_vote: false,
                can_configure: false,
                reason: eligibility.reason,
                next_step,
            })).into_response()
        }
        Err(e) => {
            tracing::error!(room_id = %room_id, "constraint check error: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse {
                error: "Failed to evaluate eligibility".to_string(),
            })).into_response()
        }
    }
}
```

Add necessary imports: `TrustRepoGraphReader`, `build_constraint`, `TrustRepo`.

**Step 4: Run tests**

Run: `cargo test -- --nocapture`
Expected: All pass.

**Step 5: Commit**

```bash
git add service/src/rooms/http/
git commit -m "feat(rooms): add my-capabilities eligibility pre-check endpoint (#755)"
```

---

### Task 6: Backend — role assignment endpoint

**Files:**
- Modify: `service/src/rooms/http/platform.rs` — add `assign_role` handler
- Modify: `service/src/rooms/http/mod.rs` — add request/response types and route

**Step 1: Add request type**

In `service/src/rooms/http/mod.rs`:

```rust
#[derive(Debug, Deserialize)]
pub struct AssignRoleRequest {
    pub account_id: Uuid,
    pub role: String,
}

#[derive(Debug, serde::Serialize)]
pub struct AssignRoleResponse {
    pub room_id: Uuid,
    pub account_id: Uuid,
    pub role: String,
}
```

**Step 2: Add route**

```rust
.route("/rooms/{room_id}/roles", post(platform::assign_role))
```

**Step 3: Implement handler**

In `service/src/rooms/http/platform.rs`:

```rust
pub async fn assign_role(
    Extension(service): Extension<Arc<dyn RoomsService>>,
    Extension(pool): Extension<PgPool>,
    Path(room_id): Path<Uuid>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let req: AssignRoleRequest = match auth.json() {
        Ok(r) => r,
        Err(resp) => return resp,
    };

    // Only the room owner can assign roles
    let room = match service.get_room(room_id).await {
        Ok(r) => r,
        Err(e) => return room_error_response(e),
    };

    if room.owner_id != Some(auth.account_id) {
        return (StatusCode::FORBIDDEN, Json(ErrorResponse {
            error: "Only the room owner can assign roles".to_string(),
        })).into_response();
    }

    // Upsert role assignment
    match sqlx::query(
        "INSERT INTO rooms__role_assignments (room_id, account_id, role, assigned_by)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (room_id, account_id)
         DO UPDATE SET role = EXCLUDED.role, assigned_by = EXCLUDED.assigned_by, assigned_at = now()"
    )
    .bind(room_id)
    .bind(req.account_id)
    .bind(&req.role)
    .bind(auth.account_id)
    .execute(&pool)
    .await {
        Ok(_) => (StatusCode::OK, Json(AssignRoleResponse {
            room_id,
            account_id: req.account_id,
            role: req.role,
        })).into_response(),
        Err(e) => {
            tracing::error!(room_id = %room_id, "role assignment failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse {
                error: "Failed to assign role".to_string(),
            })).into_response()
        }
    }
}
```

**Step 4: Run tests**

Run: `cargo test -- --nocapture`
Expected: All pass.

**Step 5: Commit**

```bash
git add service/src/rooms/http/
git commit -m "feat(rooms): add role assignment endpoint for room owners (#755)"
```

---

### Task 7: Frontend — add eligibility status to room view

**Files:**
- Modify: `web/src/engines/polling/api/client.ts` — add `owner_id`, `constraint_type` to Room type; add `fetchMyCapabilities` API call
- Modify: `web/src/features/endorsements/types.ts` or create `web/src/engines/polling/api/types.ts` — add `MyCapabilitiesResponse` type
- Modify: `web/src/pages/Poll.page.tsx` — replace hardcoded `isVerified` gate with capabilities check

**Step 1: Update Room TypeScript type**

Add to the Room interface:
```typescript
owner_id: string | null;
constraint_type: string;
```

**Step 2: Add MyCapabilitiesResponse type and API call**

```typescript
export interface MyCapabilitiesResponse {
  role: string;
  can_vote: boolean;
  can_configure: boolean;
  reason: string | null;
  next_step: string | null;
}

export async function fetchMyCapabilities(roomId: string): Promise<MyCapabilitiesResponse> {
  return fetchClient(`/rooms/${roomId}/my-capabilities`);
}
```

Add a React Query hook `useMyCapabilities(roomId)`.

**Step 3: Replace hardcoded isVerified gate in Poll.page.tsx**

Currently (line ~139): `const canVote = isActive && isAuthenticated && isVerified;`

Replace with:
```typescript
const capabilities = useMyCapabilities(roomId);
const canVote = isActive && isAuthenticated && (capabilities.data?.can_vote ?? false);
```

Replace the verification warning/prompt with a capabilities-aware message:
```tsx
{capabilities.data?.role === 'none' && (
  <Alert color="yellow" title="Not eligible to vote">
    {capabilities.data.reason}
    {capabilities.data.next_step && (
      <Text size="sm" mt="xs">{capabilities.data.next_step}</Text>
    )}
  </Alert>
)}
```

**Step 4: Run frontend lint and tests**

Run: `cd web && yarn lint && yarn vitest --run`
Expected: All pass. Some test mocks may need updating for the new Room fields.

**Step 5: Commit**

```bash
git add web/src/
git commit -m "feat(rooms): frontend eligibility status from my-capabilities endpoint (#755)"
```

---

### Task 8: Verify full stack

**Step 1: Run full lint**

Run: `just lint`

**Step 2: Run full test suite**

Run: `just test`

**Step 3: Run codegen**

Run: `just codegen`
If files changed (likely — RoomResponse has new fields), commit them.

**Step 4: Final commit if needed**

```bash
git add -A && git commit -m "chore: codegen after room capability tiers (#755)"
```

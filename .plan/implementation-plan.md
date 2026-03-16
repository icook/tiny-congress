# Denouncement UI Component Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a denouncement section to the trust dashboard where users can file denouncements by user identifier, see active denouncements, and view their remaining budget.

**Architecture:** New "Denouncements" section on `Trust.page.tsx` with its own card. Uses the existing `POST /trust/denounce` backend endpoint. Adds `denounce` and `listDenouncements` API client functions. The section shows: budget (N/2 used), active denouncements list, and an input to file new ones. Denouncement is irreversible and the UX must reflect that gravity.

**Tech Stack:** React, Mantine (Card, TextInput, Button, Badge, Modal, Alert, Stack, Group, Text), TanStack Query

**Design decision:** Denouncements live as a separate section on the trust dashboard (not as an action on individual endorsements). Users enter a user identifier to denounce. Active denouncements are shown as a list with no withdrawal (denouncements are permanent per ADR-024 budget rules).

---

### Task 1: API Client — Denounce Function

**Files:**
- Modify: `web/src/features/trust/api/client.ts`
- Modify: `web/src/features/trust/api/queries.ts`

**Step 1: Add `denounce` API function**

In `web/src/features/trust/api/client.ts`, add:

```typescript
export async function denounce(
  targetId: string,
  reason: string,
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule,
): Promise<{ message: string }> {
  return signedFetchJson('/trust/denounce', {
    method: 'POST',
    body: JSON.stringify({ target_id: targetId, reason }),
    deviceKid,
    privateKey,
    wasmCrypto,
  });
}
```

Follow the exact pattern of the existing `endorse` function in the same file.

**Step 2: Add `listMyDenouncements` API function**

Check if a `GET /trust/denouncements/mine` endpoint exists on the backend. If not, the backend needs it — but check first. If it doesn't exist, add a backend endpoint (see sub-step below).

If the endpoint exists:

```typescript
export async function listMyDenouncements(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule,
): Promise<Denouncement[]> {
  return signedFetchJson('/trust/denouncements/mine', {
    method: 'GET',
    deviceKid,
    privateKey,
    wasmCrypto,
  });
}
```

**Sub-step: Backend — Add list denouncements endpoint if missing**

Check `service/src/trust/http/mod.rs` for a GET endpoint for denouncements. The repo has `list_denouncements_by` in `denouncements.rs`. If no HTTP handler exists:

In `service/src/trust/http/mod.rs`, add:

```rust
async fn list_my_denouncements_handler(
    auth: AuthenticatedUser,
    State(trust_repo): State<Arc<dyn TrustRepo>>,
) -> Result<Json<Vec<DenouncementResponse>>, AppError> {
    let denouncements = trust_repo.list_denouncements_by(auth.account_id).await?;
    Ok(Json(denouncements.into_iter().map(DenouncementResponse::from).collect()))
}
```

Wire it into the router as `GET /trust/denouncements/mine`.

Define `DenouncementResponse`:

```rust
#[derive(Serialize)]
struct DenouncementResponse {
    target_id: Uuid,
    target_username: String, // join with accounts table
    reason: String,
    created_at: DateTime<Utc>,
}
```

**Step 3: Add TanStack Query hooks**

In `web/src/features/trust/api/queries.ts`, add:

```typescript
export function useMyDenouncements(deviceKid: string | null, privateKey: CryptoKey | null, wasmCrypto: CryptoModule | null) {
  return useQuery({
    queryKey: ['trust-denouncements'],
    queryFn: () => listMyDenouncements(deviceKid!, privateKey!, wasmCrypto!),
    enabled: !!deviceKid && !!privateKey && !!wasmCrypto,
  });
}

export function useDenounce() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: { targetId: string; reason: string; deviceKid: string; privateKey: CryptoKey; wasmCrypto: CryptoModule }) =>
      denounce(params.targetId, params.reason, params.deviceKid, params.privateKey, params.wasmCrypto),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['trust-denouncements'] });
      queryClient.invalidateQueries({ queryKey: ['trust-budget'] });
      queryClient.invalidateQueries({ queryKey: ['trust-scores'] });
    },
  });
}
```

**Step 4: Export from barrel**

Update `web/src/features/trust/api/index.ts` to export the new functions and hooks.

**Step 5: Commit**

```bash
git add web/src/features/trust/api/ service/src/trust/http/mod.rs
git commit -m "feat(trust): add denouncement API client and list endpoint (#658)"
```

---

### Task 2: Denouncement Section Component

**Files:**
- Create: `web/src/features/trust/components/DenouncementSection.tsx`

**Step 1: Write the component**

```tsx
import { Alert, Badge, Button, Card, Group, Modal, Stack, Text, TextInput, Title } from '@mantine/core';
import { useDisclosure } from '@mantine/hooks';
import { IconAlertTriangle } from '@tabler/icons-react';
import { useState } from 'react';
// Import hooks and types from trust API

interface DenouncementSectionProps {
  deviceKid: string | null;
  privateKey: CryptoKey | null;
  wasmCrypto: CryptoModule | null;
  budget: { denouncements_total: number; denouncements_used: number; denouncements_available: number } | null;
}

export function DenouncementSection({ deviceKid, privateKey, wasmCrypto, budget }: DenouncementSectionProps) {
  const [targetUsername, setTargetUsername] = useState('');
  const [reason, setReason] = useState('');
  const [confirmOpened, { open: openConfirm, close: closeConfirm }] = useDisclosure(false);

  const { data: denouncements, isLoading } = useMyDenouncements(deviceKid, privateKey, wasmCrypto);
  const denounceMutation = useDenounce();

  const handleDenounce = async () => {
    // Resolve username to account ID (may need a lookup endpoint or use target_id directly)
    // Call denounceMutation.mutate(...)
    closeConfirm();
    setTargetUsername('');
    setReason('');
  };

  return (
    <Card withBorder>
      <Stack>
        <Group justify="space-between">
          <Title order={4}>Denouncements</Title>
          {budget && (
            <Badge color={budget.denouncements_available > 0 ? 'gray' : 'red'}>
              {budget.denouncements_used}/{budget.denouncements_total} used
            </Badge>
          )}
        </Group>

        {/* Active denouncements list */}
        {denouncements?.length ? (
          denouncements.map((d) => (
            <Group key={d.target_id} justify="space-between">
              <Text size="sm">{d.target_username}</Text>
              <Text size="xs" c="dimmed">{new Date(d.created_at).toLocaleDateString()}</Text>
            </Group>
          ))
        ) : (
          <Text size="sm" c="dimmed">No active denouncements</Text>
        )}

        {/* File new denouncement */}
        {budget && budget.denouncements_available > 0 && (
          <>
            <TextInput
              label="Username to denounce"
              placeholder="Enter username"
              value={targetUsername}
              onChange={(e) => setTargetUsername(e.currentTarget.value)}
            />
            <TextInput
              label="Reason"
              placeholder="Why are you withdrawing trust?"
              value={reason}
              onChange={(e) => setReason(e.currentTarget.value)}
            />
            <Button
              color="red"
              variant="outline"
              onClick={openConfirm}
              disabled={!targetUsername.trim() || !reason.trim()}
            >
              File Denouncement
            </Button>
          </>
        )}

        {budget && budget.denouncements_available === 0 && (
          <Alert color="yellow" icon={<IconAlertTriangle size={16} />}>
            You have used all denouncement slots. This cannot be undone.
          </Alert>
        )}

        {/* Confirmation modal */}
        <Modal opened={confirmOpened} onClose={closeConfirm} title="Confirm Denouncement">
          <Stack>
            <Alert color="red" icon={<IconAlertTriangle size={16} />}>
              <Text size="sm" fw={700}>This action is irreversible.</Text>
              <Text size="sm">
                Denouncing {targetUsername} will permanently use 1 of your {budget?.denouncements_total} denouncement slots.
                If you have endorsed this user, your endorsement will be revoked.
              </Text>
            </Alert>
            <TextInput
              label={`Type "${targetUsername}" to confirm`}
              placeholder={targetUsername}
              // Add confirmation input validation
            />
            <Group justify="flex-end">
              <Button variant="default" onClick={closeConfirm}>Cancel</Button>
              <Button color="red" onClick={handleDenounce} loading={denounceMutation.isPending}>
                Denounce
              </Button>
            </Group>
          </Stack>
        </Modal>
      </Stack>
    </Card>
  );
}
```

Key UX decisions embedded in this component:
- Budget badge shows usage prominently
- Input fields hidden when budget is exhausted
- Confirmation modal requires typing the username (prevents accidental denouncement)
- Red color scheme signals gravity
- Consequences (slot cost, endorsement revocation) stated explicitly in modal

**Step 2: Commit**

```bash
git add web/src/features/trust/components/DenouncementSection.tsx
git commit -m "feat(trust): add DenouncementSection component (#658)"
```

---

### Task 3: Wire Into Trust Dashboard

**Files:**
- Modify: `web/src/pages/Trust.page.tsx`
- Modify: `web/src/features/trust/components/index.ts`

**Step 1: Export from barrel**

Add `DenouncementSection` to `web/src/features/trust/components/index.ts`.

**Step 2: Add to Trust page**

In `web/src/pages/Trust.page.tsx`, add the `DenouncementSection` after the existing content (after "My Invites" card):

```tsx
import { DenouncementSection } from '@/features/trust';

// In the component render, after the invites card:
<DenouncementSection
  deviceKid={deviceKid}
  privateKey={privateKey}
  wasmCrypto={wasmCrypto}
  budget={budgetData}
/>
```

The `budgetData` should already be available from the existing `useTrustBudget` hook used by `TrustScoreCard`.

**Step 3: Verify manually**

Run: `just dev-frontend`
Navigate to the trust page. Verify:
- Denouncement section appears with budget badge
- Input fields are visible (budget should be 0/2 used for fresh account)
- Confirmation modal works
- After denouncing: budget updates, denouncement appears in list

**Step 4: Commit**

```bash
git add web/src/pages/Trust.page.tsx web/src/features/trust/components/index.ts
git commit -m "feat(trust): wire DenouncementSection into trust dashboard (#658)"
```

---

### Task 4: Username Resolution

**Files:**
- Possibly modify: `web/src/features/trust/api/client.ts`
- Possibly modify: backend if no user lookup endpoint exists

**Step 1: Check for user lookup endpoint**

The denounce endpoint takes `target_id` (UUID), but the UI accepts a username. Check if a `GET /accounts/by-username/:username` or similar endpoint exists. If not, one needs to be added.

If no lookup endpoint exists, add one:

Backend: `GET /accounts/lookup?username=<username>` → `{ id: Uuid, username: String }`

Frontend: Add a `lookupUser` function and call it before submitting the denouncement.

**Step 2: Wire into DenouncementSection**

Update `handleDenounce` to:
1. Resolve `targetUsername` → `targetId` via lookup
2. Show error if user not found
3. Call denounce with the resolved UUID

**Step 3: Commit**

```bash
git add -A  # specific files depend on what was changed
git commit -m "feat(trust): add username resolution for denouncement target (#658)"
```

---

### Task 5: Frontend Tests

**Files:**
- Create: `web/src/features/trust/components/DenouncementSection.test.tsx`

**Step 1: Write tests**

```typescript
describe('DenouncementSection', () => {
  it('shows budget badge with usage', () => {
    // Render with budget { denouncements_total: 2, denouncements_used: 1, denouncements_available: 1 }
    // Assert: "1/2 used" badge is visible
  });

  it('hides input when budget exhausted', () => {
    // Render with budget { denouncements_available: 0 }
    // Assert: no TextInput visible
    // Assert: alert about exhausted budget is shown
  });

  it('disables button when fields empty', () => {
    // Render with available budget
    // Assert: "File Denouncement" button is disabled
  });

  it('shows confirmation modal on click', () => {
    // Fill in username and reason
    // Click "File Denouncement"
    // Assert: modal with warning text is visible
  });
});
```

Follow existing test patterns in `web/src/features/` — check `web/test-utils/` for shared mocks and providers.

**Step 2: Run tests**

Run: `cd web && yarn vitest src/features/trust/components/DenouncementSection.test.tsx --run`
Expected: PASS

**Step 3: Commit**

```bash
git add web/src/features/trust/components/DenouncementSection.test.tsx
git commit -m "test(trust): add DenouncementSection component tests (#658)"
```

---

### Task 6: Lint and Final Validation

**Step 1:** Run `just lint`
**Step 2:** Run `just test`
**Step 3:** Fix any issues.
**Step 4:** Final commit if needed.

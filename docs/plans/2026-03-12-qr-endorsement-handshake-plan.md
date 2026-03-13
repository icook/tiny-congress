# QR Endorsement Handshake Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a `/endorse` page where authenticated users create QR-based endorsement invites, scan/paste them to accept, and manage endorsement slots.

**Architecture:** New `endorsements` feature module with API client, TanStack Query hooks, and four components (GiveTab, AcceptTab, SlotCounter, EndorsementList). Single tabbed page under auth-required layout. Backend change: `accept_invite_handler` auto-enqueues endorsement on accept.

**Tech Stack:** `qr-scanner` (scanning), `qrcode.react` (generation), Mantine UI, TanStack Query/Router, Ed25519 signed requests.

**Design doc:** `docs/plans/2026-03-12-qr-endorsement-handshake-design.md`

---

### Task 1: Backend — Auto-endorse on invite accept

The `accept_invite_handler` currently only marks the invite as accepted. It must also enqueue an endorsement action so a trust edge is created.

**Files:**
- Modify: `service/src/trust/http/mod.rs:359-396` (accept_invite_handler)

**Step 1: Write the failing test**

Add a test to `service/tests/trust_http_tests.rs` that:
1. Creates two users (endorser + acceptor), each with device keys
2. Endorser creates an invite via `POST /trust/invites`
3. Acceptor accepts via `POST /trust/invites/{id}/accept`
4. Asserts that the trust action queue has a pending `"endorse"` action with `endorser_id` as the actor

```rust
#[tokio::test]
async fn accept_invite_enqueues_endorsement() {
    let ctx = TestContext::new().await;
    let (endorser, endorser_device) = ctx.create_user_with_device("endorser").await;
    let (acceptor, acceptor_device) = ctx.create_user_with_device("acceptor").await;

    // Endorser creates invite
    let envelope = base64url::encode(b"spike-test");
    let body = json!({
        "envelope": envelope,
        "delivery_method": "qr",
        "attestation": { "method": "physical_qr" }
    });
    let res = ctx.signed_post(&endorser_device, "/trust/invites", &body).await;
    assert_eq!(res.status(), 201);
    let invite: serde_json::Value = res.json().await;
    let invite_id = invite["id"].as_str().unwrap();

    // Acceptor accepts
    let res = ctx.signed_post_empty(&acceptor_device, &format!("/trust/invites/{invite_id}/accept")).await;
    assert_eq!(res.status(), 200);

    // Check action queue has endorsement
    let actions = sqlx::query_as::<_, (String, serde_json::Value)>(
        "SELECT action_type, payload FROM trust__action_queue WHERE actor_id = $1 AND status = 'pending'"
    )
    .bind(endorser.id)
    .fetch_all(&ctx.pool)
    .await
    .unwrap();

    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].0, "endorse");
    assert_eq!(actions[0].1["subject_id"], acceptor.id.to_string());
}
```

Note: Adapt this test to match the existing test harness patterns in `trust_http_tests.rs`. Check how `TestContext`, `create_user_with_device`, and `signed_post` are implemented — use whatever helpers exist. If there's no `signed_post_empty` for POST with no body, create one or pass an empty body.

**Step 2: Run test to verify it fails**

Run: `cargo test --test trust_http_tests accept_invite_enqueues_endorsement -- --nocapture`
Expected: FAIL — no endorsement action enqueued after accept

**Step 3: Modify accept_invite_handler**

In `service/src/trust/http/mod.rs`, the handler currently takes `Extension(trust_repo)`. It needs `Extension(trust_service)` as well to call `endorse()`.

```rust
async fn accept_invite_handler(
    Extension(trust_repo): Extension<Arc<dyn TrustRepo>>,
    Extension(trust_service): Extension<Arc<dyn TrustService>>,
    Path(invite_id): Path<Uuid>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let invite = match trust_repo.accept_invite(invite_id, auth.account_id).await {
        Ok(inv) => inv,
        Err(TrustRepoError::NotFound) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "invite not found, already accepted, or expired".to_string(),
                }),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("accept_invite error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                }),
            )
                .into_response();
        }
    };

    // Auto-enqueue endorsement — the stored signed envelope is the endorser's authorization
    if let Err(e) = trust_service
        .endorse(
            invite.endorser_id,
            auth.account_id,
            1.0,
            Some(invite.attestation.clone()),
        )
        .await
    {
        // Log but don't fail the accept — the invite is already consumed.
        // Endorsement failure (slots full, quota exceeded) is an endorser-side issue.
        tracing::warn!(
            "auto-endorse after invite accept failed for endorser={}: {e}",
            invite.endorser_id
        );
    }

    let accepted_at = match invite.accepted_at {
        Some(t) => t.to_rfc3339(),
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "invite accepted_at not set after acceptance"
                })),
            )
                .into_response()
        }
    };
    (
        StatusCode::OK,
        Json(AcceptInviteResponse {
            endorser_id: invite.endorser_id,
            accepted_at,
        }),
    )
        .into_response()
}
```

Note: Check that `invite.attestation` field type matches what `endorse()` expects (`Option<serde_json::Value>`). The `InviteRecord` struct should have `attestation: serde_json::Value` — wrap it in `Some()`.

**Step 4: Run test to verify it passes**

Run: `cargo test --test trust_http_tests accept_invite_enqueues_endorsement -- --nocapture`
Expected: PASS

**Step 5: Run full backend tests**

Run: `just test-backend`
Expected: All tests pass

**Step 6: Commit**

```bash
git add service/src/trust/http/mod.rs service/tests/trust_http_tests.rs
git commit -m "feat(trust): auto-enqueue endorsement on invite accept

The signed invite envelope stored in the DB serves as endorser
authorization. When an invite is accepted, the handler now calls
trust_service.endorse() to enqueue the trust edge creation.

Refs #613"
```

---

### Task 2: Add frontend dependencies

**Files:**
- Modify: `web/package.json`

**Step 1: Install packages**

```bash
cd web && yarn add qr-scanner qrcode.react
```

**Step 2: Verify build**

```bash
cd web && yarn tsc --noEmit
```

Expected: No type errors

**Step 3: Commit**

```bash
git add web/package.json web/yarn.lock
git commit -m "deps: add qr-scanner and qrcode.react for endorsement handshake

qr-scanner: camera-based QR scanning (nimiq, actively maintained)
qrcode.react: SVG QR code generation

Refs #613"
```

---

### Task 3: Endorsements API client

**Files:**
- Create: `web/src/features/endorsements/api/client.ts`
- Create: `web/src/features/endorsements/types.ts`

**Step 1: Create types**

```typescript
// web/src/features/endorsements/types.ts

export interface BudgetResponse {
  slots_total: number;
  slots_used: number;
  slots_available: number;
  denouncements_total: number;
  denouncements_used: number;
  denouncements_available: number;
}

export interface CreateInviteResponse {
  id: string;
  expires_at: string;
}

export interface InviteResponse {
  id: string;
  delivery_method: string;
  accepted_by: string | null;
  expires_at: string;
  accepted_at: string | null;
}

export interface InvitesListResponse {
  invites: InviteResponse[];
}

export interface AcceptInviteResponse {
  endorser_id: string;
  accepted_at: string;
}

export interface CreateInvitePayload {
  envelope: string;
  delivery_method: string;
  attestation: Record<string, unknown>;
}
```

**Step 2: Create API client**

```typescript
// web/src/features/endorsements/api/client.ts

import { signedFetchJson } from '@/api/signing';
import type { CryptoModule } from '@/providers/CryptoProvider';
import type {
  AcceptInviteResponse,
  BudgetResponse,
  CreateInvitePayload,
  CreateInviteResponse,
  InvitesListResponse,
} from '../types';

export async function getTrustBudget(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<BudgetResponse> {
  return signedFetchJson('/trust/budget', 'GET', deviceKid, privateKey, wasmCrypto);
}

export async function getMyInvites(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<InvitesListResponse> {
  return signedFetchJson('/trust/invites/mine', 'GET', deviceKid, privateKey, wasmCrypto);
}

export async function createInvite(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule,
  payload: CreateInvitePayload
): Promise<CreateInviteResponse> {
  return signedFetchJson('/trust/invites', 'POST', deviceKid, privateKey, wasmCrypto, payload);
}

export async function acceptInvite(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule,
  inviteId: string
): Promise<AcceptInviteResponse> {
  return signedFetchJson(
    `/trust/invites/${inviteId}/accept`,
    'POST',
    deviceKid,
    privateKey,
    wasmCrypto
  );
}

export async function revokeEndorsement(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule,
  subjectId: string
): Promise<void> {
  await signedFetchJson('/trust/revoke', 'POST', deviceKid, privateKey, wasmCrypto, {
    subject_id: subjectId,
  });
}
```

**Step 3: Verify types compile**

Run: `cd web && yarn tsc --noEmit`
Expected: No errors

**Step 4: Commit**

```bash
git add web/src/features/endorsements/types.ts web/src/features/endorsements/api/client.ts
git commit -m "feat(endorsements): API client for trust endpoints

Typed client functions for budget, invites, accept, and revoke.

Refs #613"
```

---

### Task 4: TanStack Query hooks

**Files:**
- Create: `web/src/features/endorsements/api/queries.ts`
- Create: `web/src/features/endorsements/api/index.ts`
- Create: `web/src/features/endorsements/index.ts`

**Step 1: Create query hooks**

```typescript
// web/src/features/endorsements/api/queries.ts

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import type { CryptoModule } from '@/providers/CryptoProvider';
import {
  acceptInvite,
  createInvite,
  getMyInvites,
  getTrustBudget,
  revokeEndorsement,
} from './client';
import type { CreateInvitePayload } from '../types';
import { getMyEndorsements } from '@/features/verification/api/client';

export function useTrustBudget(
  deviceKid: string | null,
  privateKey: CryptoKey | null,
  crypto: CryptoModule | undefined
) {
  return useQuery({
    queryKey: ['trust-budget', deviceKid],
    queryFn: () => getTrustBudget(deviceKid!, privateKey!, crypto!),
    enabled: Boolean(deviceKid && privateKey && crypto),
    staleTime: 30_000,
  });
}

export function useMyEndorsementsList(
  deviceKid: string | null,
  privateKey: CryptoKey | null,
  crypto: CryptoModule | undefined
) {
  return useQuery({
    queryKey: ['my-endorsements', deviceKid],
    queryFn: () => getMyEndorsements(deviceKid!, privateKey!, crypto!),
    enabled: Boolean(deviceKid && privateKey && crypto),
    staleTime: 30_000,
  });
}

export function useMyInvites(
  deviceKid: string | null,
  privateKey: CryptoKey | null,
  crypto: CryptoModule | undefined
) {
  return useQuery({
    queryKey: ['my-invites', deviceKid],
    queryFn: () => getMyInvites(deviceKid!, privateKey!, crypto!),
    enabled: Boolean(deviceKid && privateKey && crypto),
    staleTime: 30_000,
  });
}

export function useCreateInvite(
  deviceKid: string,
  privateKey: CryptoKey,
  crypto: CryptoModule
) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (payload: CreateInvitePayload) =>
      createInvite(deviceKid, privateKey, crypto, payload),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['my-invites'] });
    },
  });
}

export function useAcceptInvite(
  deviceKid: string,
  privateKey: CryptoKey,
  crypto: CryptoModule
) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (inviteId: string) =>
      acceptInvite(deviceKid, privateKey, crypto, inviteId),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['my-endorsements'] });
      void queryClient.invalidateQueries({ queryKey: ['my-invites'] });
      void queryClient.invalidateQueries({ queryKey: ['trust-budget'] });
      void queryClient.invalidateQueries({ queryKey: ['verification-status'] });
    },
  });
}

export function useRevokeEndorsement(
  deviceKid: string,
  privateKey: CryptoKey,
  crypto: CryptoModule
) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (subjectId: string) =>
      revokeEndorsement(deviceKid, privateKey, crypto, subjectId),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['my-endorsements'] });
      void queryClient.invalidateQueries({ queryKey: ['trust-budget'] });
    },
  });
}
```

**Step 2: Create barrel exports**

```typescript
// web/src/features/endorsements/api/index.ts

export {
  useTrustBudget,
  useMyEndorsementsList,
  useMyInvites,
  useCreateInvite,
  useAcceptInvite,
  useRevokeEndorsement,
} from './queries';
```

```typescript
// web/src/features/endorsements/index.ts

export {
  useTrustBudget,
  useMyEndorsementsList,
  useMyInvites,
  useCreateInvite,
  useAcceptInvite,
  useRevokeEndorsement,
} from './api';
export type {
  BudgetResponse,
  CreateInviteResponse,
  InviteResponse,
  AcceptInviteResponse,
  CreateInvitePayload,
} from './types';
```

**Step 3: Verify types compile**

Run: `cd web && yarn tsc --noEmit`
Expected: No errors

**Step 4: Commit**

```bash
git add web/src/features/endorsements/
git commit -m "feat(endorsements): TanStack Query hooks and barrel exports

Query hooks for budget, endorsements, invites with mutation
cache invalidation. Barrel exports for the endorsements feature.

Refs #613"
```

---

### Task 5: SlotCounter component

**Files:**
- Create: `web/src/features/endorsements/components/SlotCounter.tsx`

**Step 1: Write the component**

```tsx
// web/src/features/endorsements/components/SlotCounter.tsx

import { Group, Progress, Text } from '@mantine/core';

interface SlotCounterProps {
  used: number;
  total: number;
}

export function SlotCounter({ used, total }: SlotCounterProps) {
  const pct = total > 0 ? (used / total) * 100 : 0;
  const color = used >= total ? 'red' : used >= total - 1 ? 'yellow' : 'green';

  return (
    <div>
      <Group justify="space-between" mb={4}>
        <Text size="sm" fw={500}>
          Endorsement slots
        </Text>
        <Text size="sm" c="dimmed">
          {used} of {total} used
        </Text>
      </Group>
      <Progress value={pct} color={color} size="sm" radius="xl" />
    </div>
  );
}
```

**Step 2: Write unit test**

```tsx
// web/src/features/endorsements/components/SlotCounter.test.tsx

import { render, screen } from '@testing-library/react';
import { MantineProvider } from '@mantine/core';
import { SlotCounter } from './SlotCounter';

function renderWithMantine(ui: React.ReactElement) {
  return render(<MantineProvider>{ui}</MantineProvider>);
}

describe('SlotCounter', () => {
  it('displays used and total slots', () => {
    renderWithMantine(<SlotCounter used={2} total={3} />);
    expect(screen.getByText('2 of 3 used')).toBeInTheDocument();
  });

  it('displays 0 of 3 when empty', () => {
    renderWithMantine(<SlotCounter used={0} total={3} />);
    expect(screen.getByText('0 of 3 used')).toBeInTheDocument();
  });
});
```

**Step 3: Run test**

Run: `cd web && yarn vitest src/features/endorsements/components/SlotCounter.test.tsx --run`
Expected: PASS

**Step 4: Commit**

```bash
git add web/src/features/endorsements/components/SlotCounter.tsx web/src/features/endorsements/components/SlotCounter.test.tsx
git commit -m "feat(endorsements): SlotCounter component

Progress bar showing endorsement slots used/total with color coding.

Refs #613"
```

---

### Task 6: EndorsementList component

**Files:**
- Create: `web/src/features/endorsements/components/EndorsementList.tsx`

**Step 1: Write the component**

```tsx
// web/src/features/endorsements/components/EndorsementList.tsx

import { ActionIcon, Card, Group, Stack, Text, Tooltip } from '@mantine/core';
import { IconTrash } from '@tabler/icons-react';
import type { Endorsement } from '@/features/verification/api/client';

interface EndorsementListProps {
  endorsements: Endorsement[];
  onRevoke: (subjectId: string) => void;
  isRevoking: boolean;
}

export function EndorsementList({ endorsements, onRevoke, isRevoking }: EndorsementListProps) {
  const activeEndorsements = endorsements.filter(
    (e) => e.topic === 'trust' && !e.revoked
  );

  if (activeEndorsements.length === 0) {
    return (
      <Text c="dimmed" ta="center" py="lg">
        No endorsements yet. Use the Give tab to endorse someone.
      </Text>
    );
  }

  return (
    <Stack gap="xs">
      {activeEndorsements.map((e) => (
        <Card key={e.id} padding="sm" withBorder>
          <Group justify="space-between" wrap="nowrap">
            <div>
              <Text size="sm" fw={500}>
                {e.subject_id}
              </Text>
              <Text size="xs" c="dimmed">
                {new Date(e.created_at).toLocaleDateString()}
              </Text>
            </div>
            <Tooltip label="Revoke endorsement">
              <ActionIcon
                variant="subtle"
                color="red"
                onClick={() => onRevoke(e.subject_id)}
                loading={isRevoking}
              >
                <IconTrash size={16} />
              </ActionIcon>
            </Tooltip>
          </Group>
        </Card>
      ))}
    </Stack>
  );
}
```

Note: The endorsement response currently returns `subject_id` (UUID), not a username. For the demo, displaying the UUID is acceptable. If there's a user lookup endpoint, enhance later.

**Step 2: Verify types compile**

Run: `cd web && yarn tsc --noEmit`
Expected: No errors

**Step 3: Commit**

```bash
git add web/src/features/endorsements/components/EndorsementList.tsx
git commit -m "feat(endorsements): EndorsementList component with revoke

Displays active trust endorsements with date and revoke button.

Refs #613"
```

---

### Task 7: GiveTab component

**Files:**
- Create: `web/src/features/endorsements/components/GiveTab.tsx`

**Step 1: Write the component**

```tsx
// web/src/features/endorsements/components/GiveTab.tsx

import { useState } from 'react';
import { Alert, Button, CopyButton, Group, Stack, Text, Tooltip } from '@mantine/core';
import { IconCheck, IconCopy, IconShare } from '@tabler/icons-react';
import { QRCodeSVG } from 'qrcode.react';
import { notifications } from '@mantine/notifications';
import type { CryptoModule } from '@/providers/CryptoProvider';
import { useCreateInvite } from '../api/queries';

interface GiveTabProps {
  deviceKid: string;
  privateKey: CryptoKey;
  crypto: CryptoModule;
  slotsAvailable: number;
}

export function GiveTab({ deviceKid, privateKey, crypto, slotsAvailable }: GiveTabProps) {
  const createInviteMutation = useCreateInvite(deviceKid, privateKey, crypto);
  const [inviteUrl, setInviteUrl] = useState<string | null>(null);
  const [expiresAt, setExpiresAt] = useState<string | null>(null);

  const handleCreate = async () => {
    try {
      const result = await createInviteMutation.mutateAsync({
        envelope: btoa('endorsement-invite'),
        delivery_method: 'qr',
        attestation: { method: 'physical_qr' },
      });
      const url = `${window.location.origin}/endorse?invite=${result.id}`;
      setInviteUrl(url);
      setExpiresAt(result.expires_at);
    } catch (e) {
      notifications.show({
        title: 'Failed to create invite',
        message: e instanceof Error ? e.message : 'Unknown error',
        color: 'red',
      });
    }
  };

  const handleShare = async () => {
    if (!inviteUrl) return;
    if (navigator.share) {
      try {
        await navigator.share({ title: 'TinyCongress Endorsement', url: inviteUrl });
      } catch {
        // User cancelled share
      }
    }
  };

  if (slotsAvailable <= 0) {
    return (
      <Alert color="yellow" title="No slots available">
        All endorsement slots used. Revoke an existing endorsement to endorse someone new.
      </Alert>
    );
  }

  return (
    <Stack align="center" gap="md" py="md">
      {!inviteUrl ? (
        <Button
          onClick={handleCreate}
          loading={createInviteMutation.isPending}
          size="lg"
        >
          Create Endorsement Invite
        </Button>
      ) : (
        <>
          <QRCodeSVG value={inviteUrl} size={250} level="M" />
          <Text size="xs" c="dimmed" ta="center" maw={300} style={{ wordBreak: 'break-all' }}>
            {inviteUrl}
          </Text>
          {expiresAt && (
            <Text size="xs" c="dimmed">
              Expires {new Date(expiresAt).toLocaleDateString()}
            </Text>
          )}
          <Group>
            <CopyButton value={inviteUrl}>
              {({ copied, copy }) => (
                <Tooltip label={copied ? 'Copied' : 'Copy link'}>
                  <Button
                    variant="light"
                    leftSection={copied ? <IconCheck size={16} /> : <IconCopy size={16} />}
                    onClick={copy}
                    color={copied ? 'teal' : undefined}
                  >
                    {copied ? 'Copied' : 'Copy Link'}
                  </Button>
                </Tooltip>
              )}
            </CopyButton>
            {typeof navigator.share === 'function' && (
              <Button variant="light" leftSection={<IconShare size={16} />} onClick={handleShare}>
                Share
              </Button>
            )}
          </Group>
          <Button variant="subtle" onClick={() => setInviteUrl(null)}>
            Create Another
          </Button>
        </>
      )}
    </Stack>
  );
}
```

Note: The `envelope` field expects base64url-encoded bytes. For the demo, `btoa('endorsement-invite')` is fine — it's the signed intent. If the design requires the envelope to contain a proper cryptographic signature, that's a follow-up. Check what the backend validates for the envelope field — the current `create_invite_handler` just base64url-decodes it and stores the bytes.

**Step 2: Verify types compile**

Run: `cd web && yarn tsc --noEmit`
Expected: No errors

**Step 3: Commit**

```bash
git add web/src/features/endorsements/components/GiveTab.tsx
git commit -m "feat(endorsements): GiveTab component with QR generation

Creates invite, displays 250px QR code SVG, copy link, and Web Share
API integration. Disables when no slots available.

Refs #613"
```

---

### Task 8: AcceptTab component

**Files:**
- Create: `web/src/features/endorsements/components/AcceptTab.tsx`

**Step 1: Write the component**

```tsx
// web/src/features/endorsements/components/AcceptTab.tsx

import { useCallback, useEffect, useRef, useState } from 'react';
import { Alert, Button, Divider, Group, Stack, Text, TextInput } from '@mantine/core';
import { IconCamera, IconCameraOff, IconCheck } from '@tabler/icons-react';
import { notifications } from '@mantine/notifications';
import QrScanner from 'qr-scanner';
import type { CryptoModule } from '@/providers/CryptoProvider';
import { useAcceptInvite } from '../api/queries';

interface AcceptTabProps {
  deviceKid: string;
  privateKey: CryptoKey;
  crypto: CryptoModule;
  prefillInviteId?: string;
}

function extractInviteId(input: string): string | null {
  // Try URL pattern first: /endorse?invite={uuid}
  try {
    const url = new URL(input, window.location.origin);
    const invite = url.searchParams.get('invite');
    if (invite) return invite;
  } catch {
    // Not a URL
  }
  // Try bare UUID
  const uuidMatch = input.match(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/i);
  return uuidMatch ? uuidMatch[0] : null;
}

export function AcceptTab({ deviceKid, privateKey, crypto, prefillInviteId }: AcceptTabProps) {
  const acceptMutation = useAcceptInvite(deviceKid, privateKey, crypto);
  const [pasteValue, setPasteValue] = useState('');
  const [scanning, setScanning] = useState(false);
  const [accepted, setAccepted] = useState(false);
  const [acceptedEndorser, setAcceptedEndorser] = useState<string | null>(null);
  const videoRef = useRef<HTMLVideoElement>(null);
  const scannerRef = useRef<QrScanner | null>(null);

  const handleAccept = useCallback(
    async (inviteId: string) => {
      try {
        const result = await acceptMutation.mutateAsync(inviteId);
        setAccepted(true);
        setAcceptedEndorser(result.endorser_id);
        notifications.show({
          title: 'Endorsement received!',
          message: `Endorsed by ${result.endorser_id}`,
          color: 'green',
        });
      } catch (e) {
        const msg = e instanceof Error ? e.message : 'Unknown error';
        const display = msg.includes('not found')
          ? 'This invite has expired or was already used.'
          : msg;
        notifications.show({
          title: 'Failed to accept',
          message: display,
          color: 'red',
        });
      }
    },
    [acceptMutation]
  );

  // Auto-accept prefilled invite
  useEffect(() => {
    if (prefillInviteId && !accepted) {
      void handleAccept(prefillInviteId);
    }
    // Only run on mount with prefillInviteId
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [prefillInviteId]);

  const startScanner = useCallback(async () => {
    if (!videoRef.current) return;
    setScanning(true);

    const scanner = new QrScanner(
      videoRef.current,
      (result) => {
        const inviteId = extractInviteId(result.data);
        if (inviteId) {
          scanner.stop();
          scanner.destroy();
          scannerRef.current = null;
          setScanning(false);
          void handleAccept(inviteId);
        }
      },
      {
        preferredCamera: 'environment',
        highlightScanRegion: true,
        highlightCodeOutline: true,
      }
    );

    scannerRef.current = scanner;
    try {
      await scanner.start();
    } catch {
      setScanning(false);
      notifications.show({
        title: 'Camera error',
        message: 'Could not access camera. Use the paste option below.',
        color: 'yellow',
      });
    }
  }, [handleAccept]);

  const stopScanner = useCallback(() => {
    if (scannerRef.current) {
      scannerRef.current.stop();
      scannerRef.current.destroy();
      scannerRef.current = null;
    }
    setScanning(false);
  }, []);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (scannerRef.current) {
        scannerRef.current.stop();
        scannerRef.current.destroy();
      }
    };
  }, []);

  const handlePaste = () => {
    const inviteId = extractInviteId(pasteValue.trim());
    if (!inviteId) {
      notifications.show({
        title: 'Invalid link',
        message: 'Could not find an invite ID in the pasted text.',
        color: 'red',
      });
      return;
    }
    void handleAccept(inviteId);
  };

  if (accepted) {
    return (
      <Stack align="center" py="lg">
        <IconCheck size={48} color="var(--mantine-color-green-6)" />
        <Text size="lg" fw={600}>
          Endorsement received!
        </Text>
        {acceptedEndorser && (
          <Text size="sm" c="dimmed">
            From {acceptedEndorser}
          </Text>
        )}
        <Button variant="subtle" onClick={() => setAccepted(false)}>
          Accept Another
        </Button>
      </Stack>
    );
  }

  return (
    <Stack gap="md" py="md">
      {/* Camera scanner */}
      <Stack align="center" gap="sm">
        <video
          ref={videoRef}
          style={{
            width: '100%',
            maxWidth: 350,
            borderRadius: 8,
            display: scanning ? 'block' : 'none',
          }}
        />
        {!scanning ? (
          <Button
            leftSection={<IconCamera size={18} />}
            onClick={startScanner}
            loading={acceptMutation.isPending}
            size="lg"
          >
            Scan QR Code
          </Button>
        ) : (
          <Button
            leftSection={<IconCameraOff size={18} />}
            onClick={stopScanner}
            color="red"
            variant="light"
          >
            Stop Scanner
          </Button>
        )}
      </Stack>

      <Divider label="or paste an invite link" labelPosition="center" />

      {/* Paste input */}
      <Group gap="xs" align="flex-end">
        <TextInput
          placeholder="Paste invite link or ID"
          value={pasteValue}
          onChange={(e) => setPasteValue(e.currentTarget.value)}
          style={{ flex: 1 }}
        />
        <Button
          onClick={handlePaste}
          loading={acceptMutation.isPending}
          disabled={!pasteValue.trim()}
        >
          Accept
        </Button>
      </Group>

      {acceptMutation.isError && (
        <Alert color="red" title="Error">
          {acceptMutation.error instanceof Error
            ? acceptMutation.error.message
            : 'Failed to accept invite'}
        </Alert>
      )}
    </Stack>
  );
}
```

**Step 2: Verify types compile**

Run: `cd web && yarn tsc --noEmit`
Expected: No errors

**Step 3: Commit**

```bash
git add web/src/features/endorsements/components/AcceptTab.tsx
git commit -m "feat(endorsements): AcceptTab component with QR scanner and paste

Camera scanning via qr-scanner, paste invite link fallback,
auto-accept via prefilled invite ID from URL params.

Refs #613"
```

---

### Task 9: Endorse page + route registration

**Files:**
- Create: `web/src/pages/Endorse.page.tsx`
- Modify: `web/src/Router.tsx`
- Modify: `web/src/components/Navbar/Navbar.tsx`

**Step 1: Create the page component**

```tsx
// web/src/pages/Endorse.page.tsx

import { Alert, Card, Loader, Stack, Tabs, Title } from '@mantine/core';
import { IconHandGrab, IconQrcode } from '@tabler/icons-react';
import { useSearch } from '@tanstack/react-router';
import { useCryptoRequired } from '@/providers/CryptoProvider';
import { useDevice } from '@/providers/DeviceProvider';
import {
  useMyEndorsementsList,
  useRevokeEndorsement,
  useTrustBudget,
} from '@/features/endorsements';
import { SlotCounter } from '@/features/endorsements/components/SlotCounter';
import { GiveTab } from '@/features/endorsements/components/GiveTab';
import { AcceptTab } from '@/features/endorsements/components/AcceptTab';
import { EndorsementList } from '@/features/endorsements/components/EndorsementList';

export function EndorsePage() {
  const { deviceKid, privateKey } = useDevice();
  const crypto = useCryptoRequired();
  const search = useSearch({ strict: false }) as { invite?: string };

  const budgetQuery = useTrustBudget(deviceKid, privateKey, crypto);
  const endorsementsQuery = useMyEndorsementsList(deviceKid, privateKey, crypto);
  const revokeMutation = useRevokeEndorsement(deviceKid!, privateKey!, crypto);

  const defaultTab = search.invite ? 'accept' : 'give';

  if (!deviceKid || !privateKey) {
    return <Alert color="red">Not authenticated</Alert>;
  }

  return (
    <Stack gap="md" maw={500} mx="auto" py="md" px="md">
      <Title order={2}>Endorse</Title>

      {/* Slot counter */}
      {budgetQuery.isLoading ? (
        <Loader size="sm" />
      ) : budgetQuery.data ? (
        <SlotCounter
          used={budgetQuery.data.slots_used}
          total={budgetQuery.data.slots_total}
        />
      ) : null}

      {/* Tabs */}
      <Card withBorder padding="md">
        <Tabs defaultValue={defaultTab}>
          <Tabs.List grow>
            <Tabs.Tab value="give" leftSection={<IconQrcode size={16} />}>
              Give Endorsement
            </Tabs.Tab>
            <Tabs.Tab value="accept" leftSection={<IconHandGrab size={16} />}>
              Accept Endorsement
            </Tabs.Tab>
          </Tabs.List>

          <Tabs.Panel value="give" pt="md">
            <GiveTab
              deviceKid={deviceKid}
              privateKey={privateKey}
              crypto={crypto}
              slotsAvailable={budgetQuery.data?.slots_available ?? 0}
            />
          </Tabs.Panel>

          <Tabs.Panel value="accept" pt="md">
            <AcceptTab
              deviceKid={deviceKid}
              privateKey={privateKey}
              crypto={crypto}
              prefillInviteId={search.invite}
            />
          </Tabs.Panel>
        </Tabs>
      </Card>

      {/* Endorsement list */}
      <Card withBorder padding="md">
        <Title order={4} mb="sm">
          Active Endorsements
        </Title>
        {endorsementsQuery.isLoading ? (
          <Loader size="sm" />
        ) : endorsementsQuery.data ? (
          <EndorsementList
            endorsements={endorsementsQuery.data.endorsements}
            onRevoke={(subjectId) => revokeMutation.mutate(subjectId)}
            isRevoking={revokeMutation.isPending}
          />
        ) : (
          <Alert color="red">Failed to load endorsements</Alert>
        )}
      </Card>
    </Stack>
  );
}
```

**Step 2: Add route to Router.tsx**

In `web/src/Router.tsx`:

1. Add import: `import { EndorsePage } from './pages/Endorse.page';`

2. Add route definition (after `settingsRoute`):

```typescript
const endorseRoute = createRoute({
  getParentRoute: () => authRequiredLayout,
  path: 'endorse',
  component: EndorsePage,
  validateSearch: (
    search: Record<string, unknown>
  ): { invite?: string } => ({
    invite: typeof search.invite === 'string' ? search.invite : undefined,
  }),
});
```

3. Add `endorseRoute` to the auth-required children (line 128):

```typescript
authRequiredLayout.addChildren([settingsRoute, verifyCallbackRoute, endorseRoute]),
```

**Step 3: Add nav link**

In `web/src/components/Navbar/Navbar.tsx`:

1. Add import: `import { IconHandshake } from '@tabler/icons-react';`

2. Add to `navLinks` array (after Rooms, before About):

```typescript
const navLinks = [
  { icon: IconHome2, label: 'Home', path: '/' },
  { icon: IconDoor, label: 'Rooms', path: '/rooms' },
  { icon: IconHandshake, label: 'Endorse', path: '/endorse' },
  { icon: IconInfoCircle, label: 'About', path: '/about' },
];
```

Note: Only show the Endorse link to authenticated users. Check how the nav currently handles auth — if `navLinks` are always shown, either conditionally filter or move Endorse to the authenticated section.

Looking at the current Navbar: `navLinks` are always shown (lines 57-68), guest links show for unauthenticated users (lines 71-87), and verification badge shows for authenticated users (lines 88-113). The Endorse link should probably be in `navLinks` but only visible to authenticated users. Simplest approach: conditionally include it.

```typescript
const navLinks = [
  { icon: IconHome2, label: 'Home', path: '/' },
  { icon: IconDoor, label: 'Rooms', path: '/rooms' },
  { icon: IconInfoCircle, label: 'About', path: '/about' },
];

// Inside the component, after isAuthenticated is defined:
const allNavLinks = isAuthenticated
  ? [
      ...navLinks.slice(0, 2),
      { icon: IconHandshake, label: 'Endorse', path: '/endorse' },
      ...navLinks.slice(2),
    ]
  : navLinks;

// Use allNavLinks in the map (line 57)
```

**Step 4: Verify types compile and lint passes**

Run: `cd web && yarn tsc --noEmit && cd .. && just lint-frontend`
Expected: No errors

**Step 5: Commit**

```bash
git add web/src/pages/Endorse.page.tsx web/src/Router.tsx web/src/components/Navbar/Navbar.tsx
git commit -m "feat(endorsements): Endorse page, route, and nav link

Tabbed page with Give/Accept tabs, slot counter, endorsement list.
Route at /endorse with ?invite= search param for direct accept.
Nav link shown only to authenticated users.

Refs #613"
```

---

### Task 10: Update barrel exports and lint

**Files:**
- Modify: `web/src/features/endorsements/index.ts` — ensure components are exported if needed by page

**Step 1: Run full lint and type check**

Run: `just lint`
Expected: All pass. Fix any lint issues (unused imports, prefer-nullish-coalescing, etc.)

**Step 2: Run frontend tests**

Run: `just test-frontend`
Expected: Existing tests pass, SlotCounter test passes

**Step 3: Run backend tests**

Run: `just test-backend`
Expected: All pass including new accept_invite_enqueues_endorsement test

**Step 4: Commit any fixes**

```bash
git add -u
git commit -m "fix: lint and test fixes for endorsement feature

Refs #613"
```

---

### Task 11: Manual smoke test

**Step 1: Start dev server**

Run: `just dev-frontend`

**Step 2: Verify navigation**

- Log in → confirm "Endorse" appears in nav
- Click "Endorse" → lands on /endorse
- See slot counter (may show 0/3 if backend not running)
- Tabs switch between Give and Accept
- Log out → "Endorse" disappears from nav

**Step 3: Test URL param**

Navigate to `/endorse?invite=test-uuid` → should auto-switch to Accept tab

**Step 4: Test QR generation (with backend)**

- Click "Create Endorsement Invite" → QR code appears
- Copy link button works
- Share button appears on supported browsers

**Step 5: Test accept flow (with backend, two sessions)**

- User A creates invite on /endorse
- User B navigates to copied link → auto-accepts
- Or: User B scans QR from User A's screen

---

### Task 12: Push and update PR

**Step 1: Push branch**

```bash
git push origin feature/613-m4-qr-handshake-spike
```

**Step 2: Update PR description**

Update PR #618 to reflect the full implementation (spike + design + implementation). Mark as ready for review if CI passes.

---

## Dependency Graph

```
Task 1 (backend) ──────────────────────────────────────────┐
Task 2 (deps) ─────┐                                       │
Task 3 (API client) ┤                                      │
Task 4 (hooks) ─────┤                                      │
Task 5 (SlotCounter) ┤                                     │
Task 6 (EndorsementList) ┤                                 │
Task 7 (GiveTab) ────────┤                                 │
Task 8 (AcceptTab) ───────┤                                │
                          ├── Task 9 (page + route) ── Task 10 (lint) ── Task 11 (smoke) ── Task 12 (push)
```

Tasks 1-8 are independent and can be parallelized. Task 9 depends on all of them. Tasks 10-12 are sequential.

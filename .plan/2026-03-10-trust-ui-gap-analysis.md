# Trust UI Gap Analysis & Integration Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Close the gap between the shipped backend trust engine (PR #555) and the frontend — via contract validation, expanded E2E smoke tests, trust-aware frontend UI, and the QR handshake flow.

**Architecture:** Four sequential milestones. M1 validates that the migration didn't break existing flows and establishes regression baselines. M2 expands Playwright coverage to the full user journey. M3 builds the trust-aware frontend (scores, budget, invites). M4 implements the QR handshake end-to-end. Each milestone produces a shippable, testable increment.

**Tech Stack:** Rust/axum backend, React/Mantine/TanStack Router frontend, Playwright E2E tests, OpenAPI codegen via `openapi-typescript`, Ed25519 signing via WebCrypto + tc-crypto WASM.

---

## Context for the Implementer

### What just happened
PR #555 (`9c26720`) landed a ~6,000-line backend trust engine: migrations, recursive CTE distance computation, path diversity, influence budgets, action queue + worker, denouncements, invites, room constraints, and REST endpoints. The frontend was NOT updated — it still shows the pre-trust demo (signup → verify → vote).

### Key files you'll touch repeatedly
- `web/src/features/rooms/api/client.ts` — Room type, API calls
- `web/src/features/verification/` — Verification status hooks
- `web/src/pages/Poll.page.tsx` — Voting page with eligibility gates
- `web/src/components/Navbar/Navbar.tsx` — Navigation with verification badge
- `web/src/Router.tsx` — Route definitions
- `web/src/config.ts` — Runtime config (`window.__TC_ENV__`)
- `web/src/api/signing.ts` — Authenticated request signing
- `web/openapi.json` — OpenAPI spec (source for codegen)
- `web/tests/e2e/` — Playwright tests
- `service/src/trust/http/mod.rs` — Trust REST endpoints
- `service/src/trust/constraints.rs` — Room constraint implementations

### How auth works in the frontend
1. Device has a non-extractable Ed25519 `CryptoKey` in IndexedDB
2. `signedFetchJson()` builds canonical `METHOD\nPATH\nTIMESTAMP\nNONCE\nBODY_HASH` and signs it
3. Sends `X-Device-Kid`, `X-Signature`, `X-Timestamp`, `X-Nonce` headers
4. Backend validates signature, checks device isn't revoked

### How verification works today
- Frontend calls `GET /me/endorsements` (signed), looks for `topic === 'identity_verified' && !revoked`
- Poll page gates voting on `isAuthenticated && isVerified`
- Backend gates voting via `build_constraint()` → `constraint.check()` in `rooms/service.rs:343-357`

### Trust endpoints available (all return 202 for mutations)
```
POST   /trust/endorse           {subject_id, weight, attestation}
POST   /trust/revoke            {subject_id}
POST   /trust/denounce          {target_id, reason, influence_cost}
GET    /trust/scores/me         → ScoreSnapshotResponse[]
GET    /trust/budget            → BudgetResponse
POST   /trust/invites           {envelope, delivery_method, attestation}
GET    /trust/invites/mine      → InviteResponse[]
POST   /trust/invites/{id}/accept
```

---

## M1: API Contract Validation & Dead Code Audit

**Purpose:** Confirm the constraint migration didn't silently break frontend assumptions. Cheap, fast, high-signal.

### Task 1.1: Regenerate OpenAPI spec and diff

The OpenAPI spec (`web/openapi.json`) is exported from Rust via `service/src/bin/export_openapi.rs`. The trust endpoints may not be included yet.

**Files:**
- Read: `service/src/bin/export_openapi.rs`
- Read: `web/openapi.json`
- Modify: `web/openapi.json` (if regeneration produces changes)
- Modify: `web/src/api/generated/rest.ts` (regenerated)

**Step 1: Regenerate the OpenAPI spec**

Run: `just codegen`
Expected: Completes. If trust endpoints aren't annotated with utoipa, the spec won't include them — that's a finding, not a failure.

**Step 2: Diff the generated output**

Run: `git diff web/openapi.json web/src/api/generated/rest.ts`
Expected: See what changed (if anything). Document findings.

**Step 3: Commit if there are meaningful changes**

```bash
git add web/openapi.json web/src/api/generated/rest.ts
git commit -m "chore: regenerate OpenAPI spec after trust engine merge"
```

### Task 1.2: Audit Room type for constraint fields

The backend `RoomRecord` now has `constraint_type: String` and `constraint_config: serde_json::Value`. The frontend `Room` interface at `web/src/features/rooms/api/client.ts:12-19` only has `eligibility_topic: string`.

**Files:**
- Modify: `web/src/features/rooms/api/client.ts:12-19`

**Step 1: Check what the backend actually returns for rooms**

Read `service/src/rooms/repo/rooms.rs` and find the SELECT queries. Confirm whether `constraint_type` and `constraint_config` are included in the JSON response for `GET /rooms` and `GET /rooms/:id`.

**Step 2: Update the Room interface if needed**

If the backend returns these fields, add them to the frontend type:

```typescript
export interface Room {
  id: string;
  name: string;
  description: string | null;
  eligibility_topic: string;
  constraint_type: string;           // "endorsed_by" | "community" | "congress"
  constraint_config: Record<string, unknown>;
  status: string;
  created_at: string;
}
```

If the backend does NOT return them (serialization skips them), note this as a gap for M3 but don't add phantom fields.

**Step 3: Run frontend type check**

Run: `just lint-typecheck`
Expected: PASS — no type errors from the Room interface change.

**Step 4: Commit**

```bash
git add web/src/features/rooms/api/client.ts
git commit -m "fix(web): align Room interface with backend constraint fields"
```

### Task 1.3: Verify eligibility gate still works

The vote eligibility path changed from endorsement-service to constraint-based. Verify the frontend error handling still works when a vote is rejected.

**Files:**
- Read: `web/src/features/rooms/api/queries.ts` — `useCastVote` error handling
- Read: `web/src/pages/Poll.page.tsx:226-230` — error display

**Step 1: Trace the error path**

Read `service/src/rooms/service.rs:359-366` to see the HTTP response for `NotEligible`. Then read `web/src/api/fetchClient.ts` to see how errors are surfaced.

**Step 2: Document the finding**

If the error format changed (e.g., different HTTP status code or response body shape), note it. If the old error path still works, confirm it.

No code changes expected — this is a read-only audit step.

### Task 1.4: Check for dead imports/references

**Step 1: Search for stale references**

```bash
# In web/src, search for old patterns that may be broken
grep -rn 'endorsement_service\|EndorsementService' web/src/
grep -rn 'eligibility_topic' web/src/
grep -rn 'HasEndorsementResponse\|checkEndorsement' web/src/
```

**Step 2: Remove dead code if found**

The `checkEndorsement` function in `web/src/features/rooms/api/client.ts:122-129` and `HasEndorsementResponse` at line 89-91 may be unused — the frontend doesn't call them anywhere visible. Verify with grep, remove if dead.

**Step 3: Run lint and tests**

Run: `just lint-frontend && just test-frontend`
Expected: PASS

**Step 4: Commit**

```bash
git add web/src/features/rooms/api/client.ts
git commit -m "chore(web): remove unused endorsement check client code"
```

---

## M2: Expanded Playwright Smoke Tests

**Purpose:** Establish regression coverage for the full user journey before adding trust UI. The existing tests cover signup, login, rooms listing, and settings — but NOT voting, verification gates, or eligibility rejection.

### Test design principles
- Use the existing `test` and `expect` from `web/tests/e2e/fixtures.ts`
- Follow the `signupUser()` helper pattern from existing tests
- Use `browser.newContext()` for fresh-device simulation
- 15s timeouts for WASM operations, 30s for Argon2id
- Attach screenshots at key states
- Tag smoke tests with `@smoke`

### Task 2.1: Extract shared test helpers

The signup helper is duplicated across test files. Extract it.

**Files:**
- Create: `web/tests/e2e/helpers.ts`
- Modify: `web/tests/e2e/signup.spec.ts`
- Modify: `web/tests/e2e/login.spec.ts`

**Step 1: Create helpers file**

```typescript
// web/tests/e2e/helpers.ts
import type { Page } from '@playwright/test';
import { expect } from './fixtures';

/**
 * Sign up a new user and wait for the success screen.
 * Returns the username used.
 */
export async function signupUser(
  page: Page,
  username?: string,
  password = 'test-password-123'
): Promise<string> {
  const name = username ?? `test-user-${String(Date.now())}`;
  await page.goto('/signup');
  await expect(page.getByLabel(/username/i)).toBeVisible();
  await page.getByLabel(/username/i).fill(name);
  await page.getByLabel(/backup password/i).fill(password);
  await page.getByRole('button', { name: /sign up/i }).click();
  await expect(page.getByText(/Account Created/i)).toBeVisible({ timeout: 15_000 });
  return name;
}
```

**Step 2: Update existing tests to import from helpers**

Replace inline signup logic in `signup.spec.ts` and `login.spec.ts` where appropriate.

**Step 3: Run existing tests to confirm no regression**

Run: `cd web && npx playwright test --project=chromium`
Expected: All existing tests pass.

**Step 4: Commit**

```bash
git add web/tests/e2e/helpers.ts web/tests/e2e/signup.spec.ts web/tests/e2e/login.spec.ts
git commit -m "refactor(e2e): extract shared signup helper"
```

### Task 2.2: Voting flow smoke test

Test the full journey: signup → navigate to room → attempt vote → see verification gate.

**Files:**
- Create: `web/tests/e2e/voting.spec.ts`

**Step 1: Write the test**

```typescript
// web/tests/e2e/voting.spec.ts
import { expect, test } from './fixtures';
import { signupUser } from './helpers';

test('unverified user sees verification gate on poll page @smoke', async ({ page }) => {
  await signupUser(page);

  // Navigate to rooms
  await page.goto('/rooms');
  await expect(page.getByText(/rooms/i)).toBeVisible();

  // Screenshot: rooms page
  await test.info().attach('rooms-page', {
    body: await page.screenshot(),
    contentType: 'image/png',
  });

  // If rooms exist, click the first poll link
  const pollLink = page.locator('a[href*="/polls/"]').first();
  if (await pollLink.isVisible({ timeout: 5_000 }).catch(() => false)) {
    await pollLink.click();

    // Should see verification gate (user is signed up but not verified)
    await expect(
      page.getByText(/verify your identity/i)
    ).toBeVisible({ timeout: 10_000 });

    // Sliders should be disabled
    const slider = page.locator('.mantine-Slider-root').first();
    if (await slider.isVisible({ timeout: 3_000 }).catch(() => false)) {
      // The slider input should be disabled
      await expect(slider.locator('input')).toBeDisabled();
    }

    // Screenshot: verification gate
    await test.info().attach('verification-gate', {
      body: await page.screenshot(),
      contentType: 'image/png',
    });
  } else {
    // No rooms/polls seeded — document this as expected in empty environment
    await test.info().attach('no-polls-available', {
      body: await page.screenshot(),
      contentType: 'image/png',
    });
  }
});

test('guest user sees login prompt on poll page @smoke', async ({ page }) => {
  // Navigate directly to rooms without signing up
  await page.goto('/rooms');

  const pollLink = page.locator('a[href*="/polls/"]').first();
  if (await pollLink.isVisible({ timeout: 5_000 }).catch(() => false)) {
    await pollLink.click();

    // Should see login/signup prompt
    await expect(
      page.getByText(/sign up/i)
    ).toBeVisible({ timeout: 10_000 });

    await test.info().attach('guest-poll-gate', {
      body: await page.screenshot(),
      contentType: 'image/png',
    });
  }
});
```

**Step 2: Run to verify**

Run: `cd web && npx playwright test voting.spec.ts --project=chromium`
Expected: Tests pass (behavior adapts to whether rooms/polls exist in the environment).

**Step 3: Commit**

```bash
git add web/tests/e2e/voting.spec.ts
git commit -m "test(e2e): add voting flow smoke tests with verification gate"
```

### Task 2.3: Visual regression baseline

Capture screenshot baselines for every major page.

**Files:**
- Create: `web/tests/e2e/visual-baseline.spec.ts`

**Step 1: Write the baseline test**

```typescript
// web/tests/e2e/visual-baseline.spec.ts
import { expect, test } from './fixtures';
import { signupUser } from './helpers';

test.describe('visual baselines', () => {
  test('home page', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByText(/TinyCongress/i)).toBeVisible();
    await expect(page).toHaveScreenshot('home.png', { maxDiffPixelRatio: 0.01 });
  });

  test('rooms page', async ({ page }) => {
    await page.goto('/rooms');
    // Wait for rooms to load (or empty state)
    await page.waitForLoadState('networkidle');
    await expect(page).toHaveScreenshot('rooms.png', { maxDiffPixelRatio: 0.01 });
  });

  test('about page', async ({ page }) => {
    await page.goto('/about');
    await page.waitForLoadState('networkidle');
    await expect(page).toHaveScreenshot('about.png', { maxDiffPixelRatio: 0.01 });
  });

  test('signup page', async ({ page }) => {
    await page.goto('/signup');
    await expect(page.getByLabel(/username/i)).toBeVisible();
    await expect(page).toHaveScreenshot('signup.png', { maxDiffPixelRatio: 0.01 });
  });

  test('login page', async ({ page }) => {
    await page.goto('/login');
    await expect(page.getByLabel(/username/i)).toBeVisible();
    await expect(page).toHaveScreenshot('login.png', { maxDiffPixelRatio: 0.01 });
  });

  test('settings page (authenticated)', async ({ page }) => {
    await signupUser(page);
    await page.goto('/settings');
    await expect(page.getByText(/devices/i)).toBeVisible({ timeout: 10_000 });
    await expect(page).toHaveScreenshot('settings.png', { maxDiffPixelRatio: 0.01 });
  });
});
```

**Step 2: Generate initial baselines**

Run: `cd web && npx playwright test visual-baseline.spec.ts --project=chromium --update-snapshots`
Expected: Creates `.png` baseline files in `web/tests/e2e/visual-baseline.spec.ts-snapshots/`.

**Step 3: Verify baselines pass**

Run: `cd web && npx playwright test visual-baseline.spec.ts --project=chromium`
Expected: All pass (comparing against just-created baselines).

**Step 4: Commit**

```bash
git add web/tests/e2e/visual-baseline.spec.ts web/tests/e2e/visual-baseline.spec.ts-snapshots/
git commit -m "test(e2e): add visual regression baselines for all pages"
```

### Task 2.4: Navigation and routing smoke test

Test that all nav links work and auth redirects function correctly.

**Files:**
- Create: `web/tests/e2e/navigation.spec.ts`

**Step 1: Write navigation tests**

```typescript
// web/tests/e2e/navigation.spec.ts
import { expect, test } from './fixtures';
import { signupUser } from './helpers';

test('guest nav links all resolve @smoke', async ({ page }) => {
  await page.goto('/');

  // Home renders
  await expect(page.getByText(/TinyCongress/i)).toBeVisible();

  // Rooms is accessible
  await page.goto('/rooms');
  await page.waitForLoadState('networkidle');

  // About is accessible
  await page.goto('/about');
  await expect(page.getByText(/about/i)).toBeVisible();

  // Settings redirects to login
  await page.goto('/settings');
  await expect(page.getByLabel(/username/i)).toBeVisible({ timeout: 5_000 });
  expect(page.url()).toContain('/login');
});

test('authenticated nav links all resolve @smoke', async ({ page }) => {
  await signupUser(page);

  // Settings is accessible after signup
  await page.goto('/settings');
  await expect(page.getByText(/devices/i)).toBeVisible({ timeout: 10_000 });

  // Login/signup redirect to rooms when authenticated
  await page.goto('/signup');
  await expect(page.url()).toContain('/rooms');

  await page.goto('/login');
  await expect(page.url()).toContain('/rooms');
});
```

**Step 2: Run tests**

Run: `cd web && npx playwright test navigation.spec.ts --project=chromium`
Expected: PASS

**Step 3: Commit**

```bash
git add web/tests/e2e/navigation.spec.ts
git commit -m "test(e2e): add navigation and auth redirect smoke tests"
```

---

## M3: Trust-Aware Frontend UI

**Purpose:** Surface the trust system to users. Display trust scores, endorsement budget, and invite management. This is the biggest gap — the backend is ready but the frontend doesn't know the trust system exists.

### Architecture decisions
- New feature module: `web/src/features/trust/` (parallel to `verification/`)
- API client calls signed REST endpoints (`/trust/scores/me`, `/trust/budget`, `/trust/invites/*`)
- TanStack Query hooks for data fetching
- Components integrate into existing pages (settings, navbar) rather than standalone pages initially
- Add a dedicated `/trust` route for the trust dashboard

### Task 3.1: Trust API client

**Files:**
- Create: `web/src/features/trust/api/client.ts`
- Create: `web/src/features/trust/api/queries.ts`
- Create: `web/src/features/trust/api/index.ts`
- Create: `web/src/features/trust/index.ts`

**Step 1: Create the trust API client**

```typescript
// web/src/features/trust/api/client.ts
import { signedFetchJson } from '@/api/signing';
import type { CryptoModule } from '@/providers/CryptoProvider';

export interface ScoreSnapshot {
  context_user_id: string | null;
  trust_distance: number | null;
  path_diversity: number;
  eigenvector_centrality: number | null;
  computed_at: string;
}

export interface TrustBudget {
  total_influence: number;
  staked_influence: number;
  spent_influence: number;
  available_influence: number;
}

export interface Invite {
  id: string;
  delivery_method: string;
  accepted_by: string | null;
  expires_at: string;
  accepted_at: string | null;
}

export interface AcceptInviteResult {
  endorser_id: string;
  accepted_at: string;
}

export async function getMyScores(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<ScoreSnapshot[]> {
  return signedFetchJson('/trust/scores/me', 'GET', deviceKid, privateKey, wasmCrypto);
}

export async function getMyBudget(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<TrustBudget> {
  return signedFetchJson('/trust/budget', 'GET', deviceKid, privateKey, wasmCrypto);
}

export async function createInvite(
  envelope: string,
  deliveryMethod: string,
  attestation: Record<string, unknown>,
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<{ id: string; expires_at: string }> {
  return signedFetchJson('/trust/invites', 'POST', deviceKid, privateKey, wasmCrypto, {
    envelope,
    delivery_method: deliveryMethod,
    attestation,
  });
}

export async function listMyInvites(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<Invite[]> {
  return signedFetchJson('/trust/invites/mine', 'GET', deviceKid, privateKey, wasmCrypto);
}

export async function acceptInvite(
  inviteId: string,
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<AcceptInviteResult> {
  return signedFetchJson(`/trust/invites/${inviteId}/accept`, 'POST', deviceKid, privateKey, wasmCrypto);
}

export async function endorse(
  subjectId: string,
  weight: number,
  attestation: Record<string, unknown> | null,
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<void> {
  await signedFetchJson('/trust/endorse', 'POST', deviceKid, privateKey, wasmCrypto, {
    subject_id: subjectId,
    weight,
    attestation,
  });
}

export async function revokeEndorsement(
  subjectId: string,
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<void> {
  await signedFetchJson('/trust/revoke', 'POST', deviceKid, privateKey, wasmCrypto, {
    subject_id: subjectId,
  });
}
```

**Step 2: Create query hooks**

```typescript
// web/src/features/trust/api/queries.ts
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import type { CryptoModule } from '@/providers/CryptoProvider';
import {
  acceptInvite,
  createInvite,
  endorse,
  getMyBudget,
  getMyScores,
  listMyInvites,
  revokeEndorsement,
  type AcceptInviteResult,
  type Invite,
  type ScoreSnapshot,
  type TrustBudget,
} from './client';

export function useTrustScores(
  deviceKid: string | null,
  privateKey: CryptoKey | null,
  wasmCrypto: CryptoModule | null
) {
  return useQuery<ScoreSnapshot[]>({
    queryKey: ['trust-scores', deviceKid],
    queryFn: () => {
      if (!deviceKid || !privateKey || !wasmCrypto) throw new Error('Not authenticated');
      return getMyScores(deviceKid, privateKey, wasmCrypto);
    },
    enabled: Boolean(deviceKid && privateKey && wasmCrypto),
    staleTime: 30_000,
  });
}

export function useTrustBudget(
  deviceKid: string | null,
  privateKey: CryptoKey | null,
  wasmCrypto: CryptoModule | null
) {
  return useQuery<TrustBudget>({
    queryKey: ['trust-budget', deviceKid],
    queryFn: () => {
      if (!deviceKid || !privateKey || !wasmCrypto) throw new Error('Not authenticated');
      return getMyBudget(deviceKid, privateKey, wasmCrypto);
    },
    enabled: Boolean(deviceKid && privateKey && wasmCrypto),
    staleTime: 30_000,
  });
}

export function useMyInvites(
  deviceKid: string | null,
  privateKey: CryptoKey | null,
  wasmCrypto: CryptoModule | null
) {
  return useQuery<Invite[]>({
    queryKey: ['trust-invites', deviceKid],
    queryFn: () => {
      if (!deviceKid || !privateKey || !wasmCrypto) throw new Error('Not authenticated');
      return listMyInvites(deviceKid, privateKey, wasmCrypto);
    },
    enabled: Boolean(deviceKid && privateKey && wasmCrypto),
  });
}

export function useCreateInvite(
  deviceKid: string | null,
  privateKey: CryptoKey | null,
  wasmCrypto: CryptoModule | null
) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (args: {
      envelope: string;
      deliveryMethod: string;
      attestation: Record<string, unknown>;
    }) => {
      if (!deviceKid || !privateKey || !wasmCrypto) throw new Error('Not authenticated');
      return createInvite(
        args.envelope,
        args.deliveryMethod,
        args.attestation,
        deviceKid,
        privateKey,
        wasmCrypto
      );
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['trust-invites'] });
      void queryClient.invalidateQueries({ queryKey: ['trust-budget'] });
    },
  });
}

export function useAcceptInvite(
  deviceKid: string | null,
  privateKey: CryptoKey | null,
  wasmCrypto: CryptoModule | null
) {
  const queryClient = useQueryClient();
  return useMutation<AcceptInviteResult, Error, string>({
    mutationFn: async (inviteId: string) => {
      if (!deviceKid || !privateKey || !wasmCrypto) throw new Error('Not authenticated');
      return acceptInvite(inviteId, deviceKid, privateKey, wasmCrypto);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['trust-scores'] });
      void queryClient.invalidateQueries({ queryKey: ['trust-budget'] });
    },
  });
}

export function useEndorse(
  deviceKid: string | null,
  privateKey: CryptoKey | null,
  wasmCrypto: CryptoModule | null
) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (args: {
      subjectId: string;
      weight: number;
      attestation: Record<string, unknown> | null;
    }) => {
      if (!deviceKid || !privateKey || !wasmCrypto) throw new Error('Not authenticated');
      return endorse(args.subjectId, args.weight, args.attestation, deviceKid, privateKey, wasmCrypto);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['trust-scores'] });
      void queryClient.invalidateQueries({ queryKey: ['trust-budget'] });
    },
  });
}

export function useRevokeEndorsement(
  deviceKid: string | null,
  privateKey: CryptoKey | null,
  wasmCrypto: CryptoModule | null
) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (subjectId: string) => {
      if (!deviceKid || !privateKey || !wasmCrypto) throw new Error('Not authenticated');
      return revokeEndorsement(subjectId, deviceKid, privateKey, wasmCrypto);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['trust-scores'] });
      void queryClient.invalidateQueries({ queryKey: ['trust-budget'] });
    },
  });
}
```

**Step 3: Create barrel exports**

```typescript
// web/src/features/trust/api/index.ts
export { getMyScores, getMyBudget, listMyInvites, createInvite, acceptInvite, endorse, revokeEndorsement } from './client';
export type { ScoreSnapshot, TrustBudget, Invite, AcceptInviteResult } from './client';
export { useTrustScores, useTrustBudget, useMyInvites, useCreateInvite, useAcceptInvite, useEndorse, useRevokeEndorsement } from './queries';
```

```typescript
// web/src/features/trust/index.ts
export * from './api';
```

**Step 4: Run lint**

Run: `just lint-frontend`
Expected: PASS

**Step 5: Commit**

```bash
git add web/src/features/trust/
git commit -m "feat(web): add trust API client and query hooks"
```

### Task 3.2: Trust score display component

A card that shows the user's current trust distance, path diversity, and influence budget.

**Files:**
- Create: `web/src/features/trust/components/TrustScoreCard.tsx`
- Create: `web/src/features/trust/components/index.ts`
- Modify: `web/src/features/trust/index.ts`

**Step 1: Write the component**

```typescript
// web/src/features/trust/components/TrustScoreCard.tsx
import { Badge, Card, Group, Loader, Progress, Stack, Text, Title } from '@mantine/core';
import { IconShieldCheck, IconRoute, IconUsers } from '@tabler/icons-react';
import { useTrustBudget, useTrustScores } from '../api';
import type { CryptoModule } from '@/providers/CryptoProvider';

interface TrustScoreCardProps {
  deviceKid: string | null;
  privateKey: CryptoKey | null;
  wasmCrypto: CryptoModule | null;
}

export function TrustScoreCard({ deviceKid, privateKey, wasmCrypto }: TrustScoreCardProps) {
  const scoresQuery = useTrustScores(deviceKid, privateKey, wasmCrypto);
  const budgetQuery = useTrustBudget(deviceKid, privateKey, wasmCrypto);

  if (!deviceKid) return null;

  if (scoresQuery.isLoading || budgetQuery.isLoading) {
    return (
      <Card shadow="sm" padding="lg" radius="md" withBorder>
        <Loader size="sm" />
      </Card>
    );
  }

  // Find the global score (no context_user_id)
  const globalScore = scoresQuery.data?.find((s) => s.context_user_id === null);
  const budget = budgetQuery.data;

  const distance = globalScore?.trust_distance;
  const diversity = globalScore?.path_diversity ?? 0;

  // Determine tier qualification
  const communityEligible = distance != null && distance <= 6.0 && diversity >= 1;
  const congressEligible = distance != null && distance <= 3.0 && diversity >= 2;

  return (
    <Card shadow="sm" padding="lg" radius="md" withBorder>
      <Stack gap="md">
        <Group justify="space-between">
          <Title order={4}>Trust Score</Title>
          {congressEligible ? (
            <Badge color="violet" variant="light">Congress</Badge>
          ) : communityEligible ? (
            <Badge color="blue" variant="light">Community</Badge>
          ) : (
            <Badge color="gray" variant="light">No tier</Badge>
          )}
        </Group>

        {distance != null ? (
          <Group gap="xl">
            <Stack gap={4} align="center">
              <IconRoute size={20} />
              <Text size="xl" fw={700}>{distance.toFixed(1)}</Text>
              <Text size="xs" c="dimmed">Distance</Text>
            </Stack>
            <Stack gap={4} align="center">
              <IconUsers size={20} />
              <Text size="xl" fw={700}>{String(diversity)}</Text>
              <Text size="xs" c="dimmed">Diversity</Text>
            </Stack>
          </Group>
        ) : (
          <Text size="sm" c="dimmed">
            No trust score yet. Get endorsed by a trusted member to join the network.
          </Text>
        )}

        {budget ? (
          <div>
            <Group justify="space-between" mb={4}>
              <Text size="sm" fw={500}>Endorsement Budget</Text>
              <Text size="sm" c="dimmed">
                {budget.available_influence.toFixed(0)} / {budget.total_influence.toFixed(0)} available
              </Text>
            </Group>
            <Progress
              value={(budget.available_influence / budget.total_influence) * 100}
              size="sm"
              radius="xl"
            />
          </div>
        ) : null}
      </Stack>
    </Card>
  );
}
```

**Step 2: Create barrel export**

```typescript
// web/src/features/trust/components/index.ts
export { TrustScoreCard } from './TrustScoreCard';
```

Update `web/src/features/trust/index.ts`:
```typescript
export * from './api';
export * from './components';
```

**Step 3: Run lint**

Run: `just lint-frontend`
Expected: PASS

**Step 4: Commit**

```bash
git add web/src/features/trust/
git commit -m "feat(web): add TrustScoreCard component"
```

### Task 3.3: Integrate trust score into Settings page

Show the TrustScoreCard on the settings page alongside device management.

**Files:**
- Modify: `web/src/pages/Settings.page.tsx`

**Step 1: Read the current Settings page**

Read `web/src/pages/Settings.page.tsx` to understand the layout.

**Step 2: Add TrustScoreCard import and render**

Add `import { TrustScoreCard } from '@/features/trust';` and render it above or below the device table.

The exact edit depends on the current page structure — find the main `<Stack>` and insert `<TrustScoreCard deviceKid={deviceKid} privateKey={privateKey} wasmCrypto={crypto} />` as a sibling of the existing cards.

**Step 3: Run lint and verify**

Run: `just lint-frontend`
Expected: PASS

**Step 4: Commit**

```bash
git add web/src/pages/Settings.page.tsx
git commit -m "feat(web): show trust score card on settings page"
```

### Task 3.4: Trust dashboard page

A dedicated page at `/trust` showing scores, budget, and invites.

**Files:**
- Create: `web/src/pages/Trust.page.tsx`
- Modify: `web/src/Router.tsx`
- Modify: `web/src/components/Navbar/Navbar.tsx`

**Step 1: Create the Trust page**

```typescript
// web/src/pages/Trust.page.tsx
import { Alert, Badge, Button, Card, Group, Loader, Stack, Table, Text, Title } from '@mantine/core';
import { IconAlertTriangle, IconSend, IconUserCheck, IconUserX } from '@tabler/icons-react';
import { TrustScoreCard, useMyInvites } from '@/features/trust';
import { useCrypto } from '@/providers/CryptoProvider';
import { useDevice } from '@/providers/DeviceProvider';

export function TrustPage() {
  const { deviceKid, privateKey } = useDevice();
  const { crypto } = useCrypto();
  const invitesQuery = useMyInvites(deviceKid, privateKey, crypto);

  return (
    <Stack gap="md" maw={800} mx="auto" mt="xl" px="md">
      <Title order={2}>Trust & Identity</Title>

      <TrustScoreCard deviceKid={deviceKid} privateKey={privateKey} wasmCrypto={crypto} />

      <Card shadow="sm" padding="lg" radius="md" withBorder>
        <Stack gap="md">
          <Group justify="space-between">
            <Title order={4}>My Invites</Title>
          </Group>

          {invitesQuery.isLoading ? <Loader size="sm" /> : null}

          {invitesQuery.isError ? (
            <Alert icon={<IconAlertTriangle size={16} />} color="red">
              Failed to load invites: {invitesQuery.error.message}
            </Alert>
          ) : null}

          {invitesQuery.data && invitesQuery.data.length === 0 ? (
            <Text size="sm" c="dimmed">
              You haven't sent any invites yet.
            </Text>
          ) : null}

          {invitesQuery.data && invitesQuery.data.length > 0 ? (
            <Table>
              <Table.Thead>
                <Table.Tr>
                  <Table.Th>Method</Table.Th>
                  <Table.Th>Status</Table.Th>
                  <Table.Th>Expires</Table.Th>
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {invitesQuery.data.map((invite) => (
                  <Table.Tr key={invite.id}>
                    <Table.Td>
                      <Badge variant="light" size="sm">
                        {invite.delivery_method}
                      </Badge>
                    </Table.Td>
                    <Table.Td>
                      {invite.accepted_at ? (
                        <Badge color="green" variant="light" leftSection={<IconUserCheck size={12} />}>
                          Accepted
                        </Badge>
                      ) : new Date(invite.expires_at) < new Date() ? (
                        <Badge color="gray" variant="light" leftSection={<IconUserX size={12} />}>
                          Expired
                        </Badge>
                      ) : (
                        <Badge color="blue" variant="light" leftSection={<IconSend size={12} />}>
                          Pending
                        </Badge>
                      )}
                    </Table.Td>
                    <Table.Td>
                      <Text size="sm">{new Date(invite.expires_at).toLocaleDateString()}</Text>
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          ) : null}
        </Stack>
      </Card>
    </Stack>
  );
}
```

**Step 2: Add route**

In `web/src/Router.tsx`, add:

```typescript
import { TrustPage } from './pages/Trust.page';
```

Add a route inside the `authRequiredLayout` children:

```typescript
const trustRoute = createRoute({
  getParentRoute: () => authRequiredLayout,
  path: 'trust',
  component: TrustPage,
});
```

Add to route tree:
```typescript
authRequiredLayout.addChildren([settingsRoute, verifyCallbackRoute, trustRoute]),
```

**Step 3: Add nav link**

In `web/src/components/Navbar/Navbar.tsx`, add an import for a trust icon and a nav entry for authenticated users. Add to the auth section (between the main nav links and the verification badge):

```typescript
import { IconShieldHalfFilled } from '@tabler/icons-react';
```

Add a NavLink to `/trust` in the authenticated section, visible only when `isAuthenticated`.

**Step 4: Run lint and type check**

Run: `just lint-frontend`
Expected: PASS

**Step 5: Commit**

```bash
git add web/src/pages/Trust.page.tsx web/src/Router.tsx web/src/components/Navbar/Navbar.tsx
git commit -m "feat(web): add trust dashboard page with scores and invites"
```

### Task 3.5: Update Navbar trust badge

Replace the simple "Verified/Unverified" badge with a trust-aware indicator that shows the user's tier.

**Files:**
- Modify: `web/src/components/Navbar/Navbar.tsx`

**Step 1: Read current navbar**

Already read — see `Navbar.tsx:88-113`. The bottom section shows a green "Verified" badge or yellow "Unverified — click to verify" badge.

**Step 2: Add trust score to navbar**

Import `useTrustScores` from `@/features/trust`. Show the tier badge (Community/Congress/No tier) alongside or replacing the verification badge. If verified AND has a trust score, show the tier. If verified but no score, show "Verified". If unverified, keep the existing yellow badge.

```typescript
// Add to imports
import { useTrustScores } from '@/features/trust';

// Inside Navbar component, after existing hooks:
const scoresQuery = useTrustScores(deviceKid, privateKey, crypto);
const globalScore = scoresQuery.data?.find((s) => s.context_user_id === null);
const distance = globalScore?.trust_distance;
const diversity = globalScore?.path_diversity ?? 0;
const congressEligible = distance != null && distance <= 3.0 && diversity >= 2;
const communityEligible = distance != null && distance <= 6.0 && diversity >= 1;
```

Replace the badge rendering in the authenticated section to show tier when available.

**Step 3: Run lint**

Run: `just lint-frontend`
Expected: PASS

**Step 4: Commit**

```bash
git add web/src/components/Navbar/Navbar.tsx
git commit -m "feat(web): show trust tier badge in navbar"
```

### Task 3.6: Update Poll page eligibility message

The Poll page currently shows a generic "verify your identity" message. With the constraint system, the backend returns specific reasons. Surface them.

**Files:**
- Modify: `web/src/pages/Poll.page.tsx`

**Step 1: Read error handling in vote mutation**

The `useCastVote` mutation surfaces errors via `voteMutation.error.message` (line 228). The backend returns RFC 7807 `ProblemDetails` with a `detail` field for `NotEligible`.

**Step 2: Check the fetchClient error handling**

Read `web/src/api/fetchClient.ts` to see how errors are parsed. The `detail` field from ProblemDetails should become the `Error.message`.

**Step 3: Improve the pre-vote eligibility display**

Instead of only showing verification status, also show the trust score context. Import `useTrustScores` and display what's needed:

```typescript
// After existing verification gate at lines 209-224, add trust context:
{isAuthenticated && isVerified && !scoresQuery.data?.length ? (
  <Alert icon={<IconShieldOff size={16} />} color="yellow">
    You're verified, but you need a trust score to vote in this room.
    Ask a trusted member to endorse you.
  </Alert>
) : null}
```

The exact logic depends on the room's constraint type. For the MVP, showing the trust score status is sufficient — the backend will still enforce the actual constraint.

**Step 4: Run lint**

Run: `just lint-frontend`
Expected: PASS

**Step 5: Commit**

```bash
git add web/src/pages/Poll.page.tsx
git commit -m "feat(web): show trust context on poll eligibility gate"
```

### Task 3.7: Add E2E test for trust dashboard

**Files:**
- Create: `web/tests/e2e/trust.spec.ts`

**Step 1: Write the test**

```typescript
// web/tests/e2e/trust.spec.ts
import { expect, test } from './fixtures';
import { signupUser } from './helpers';

test('trust page loads for authenticated user @smoke', async ({ page }) => {
  await signupUser(page);

  await page.goto('/trust');
  await expect(page.getByText(/Trust & Identity/i)).toBeVisible({ timeout: 10_000 });

  // Trust score card should render (may show "no score" for fresh user)
  await expect(page.getByText(/Trust Score/i)).toBeVisible();

  // Invites section should render
  await expect(page.getByText(/My Invites/i)).toBeVisible();

  await test.info().attach('trust-dashboard', {
    body: await page.screenshot(),
    contentType: 'image/png',
  });
});

test('trust page redirects to login when not authenticated', async ({ page }) => {
  await page.goto('/trust');
  await expect(page.getByLabel(/username/i)).toBeVisible({ timeout: 5_000 });
  expect(page.url()).toContain('/login');
});
```

**Step 2: Run the test**

Run: `cd web && npx playwright test trust.spec.ts --project=chromium`
Expected: PASS

**Step 3: Commit**

```bash
git add web/tests/e2e/trust.spec.ts
git commit -m "test(e2e): add trust dashboard smoke tests"
```

---

## M4: QR Handshake Flow

**Purpose:** Implement the physical QR handshake — the "ritual" that creates high-weight trust edges. This is the core demo moment: scan a QR code, trust score updates, Congress room unlocks.

### Architecture
- **QR Generation:** Endorser generates a short-lived invite via `POST /trust/invites`, encodes the invite ID + metadata as a QR code
- **QR Scanning:** Recipient scans QR, decodes invite ID, calls `POST /trust/invites/{id}/accept`
- **Libraries:** `qrcode.react` for generation (lightweight, React-native), `html5-qrcode` for camera scanning
- **No JWT needed for MVP:** The invite ID is the token. The accept endpoint is already authenticated via device signature. The invite record has an expiry and single-use constraint.

### Task 4.1: Add QR dependencies

**Files:**
- Modify: `web/package.json`

**Step 1: Install QR libraries**

Run: `cd web && yarn add qrcode.react html5-qrcode`

**Step 2: Verify types**

Run: `cd web && yarn add -D @types/qrcode.react` (if needed — check if types are bundled)

**Step 3: Run type check**

Run: `just lint-typecheck`
Expected: PASS

**Step 4: Commit**

```bash
git add web/package.json web/yarn.lock
git commit -m "chore(web): add qrcode.react and html5-qrcode dependencies"
```

### Task 4.2: QR code generation component

Shows a QR code containing an invite link that another user can scan.

**Files:**
- Create: `web/src/features/trust/components/QRHandshake.tsx`

**Step 1: Write the QR generation component**

```typescript
// web/src/features/trust/components/QRHandshake.tsx
import { useState } from 'react';
import { QRCodeSVG } from 'qrcode.react';
import { Alert, Button, Card, Group, Loader, Stack, Text, Title } from '@mantine/core';
import { IconAlertTriangle, IconQrcode } from '@tabler/icons-react';
import { useCreateInvite } from '../api';
import type { CryptoModule } from '@/providers/CryptoProvider';

interface QRHandshakeGeneratorProps {
  deviceKid: string;
  privateKey: CryptoKey;
  wasmCrypto: CryptoModule;
}

export function QRHandshakeGenerator({ deviceKid, privateKey, wasmCrypto }: QRHandshakeGeneratorProps) {
  const createInviteMutation = useCreateInvite(deviceKid, privateKey, wasmCrypto);
  const [inviteUrl, setInviteUrl] = useState<string | null>(null);

  const handleGenerate = () => {
    // The envelope for QR handshakes is a placeholder — the real value is
    // the invite ID which serves as the token. The accept endpoint handles
    // creating the trust edge.
    const envelope = wasmCrypto.encode_base64url(new Uint8Array([0]));
    createInviteMutation.mutate(
      {
        envelope,
        deliveryMethod: 'qr',
        attestation: { context: 'physical_qr' },
      },
      {
        onSuccess: (data) => {
          const url = `${window.location.origin}/handshake/${data.id}`;
          setInviteUrl(url);
        },
      }
    );
  };

  return (
    <Card shadow="sm" padding="lg" radius="md" withBorder>
      <Stack gap="md">
        <Group justify="space-between">
          <Title order={4}>QR Handshake</Title>
          <IconQrcode size={20} />
        </Group>

        <Text size="sm" c="dimmed">
          Generate a QR code for someone standing next to you to scan.
          This creates a high-trust endorsement.
        </Text>

        {createInviteMutation.isError ? (
          <Alert icon={<IconAlertTriangle size={16} />} color="red">
            {createInviteMutation.error.message}
          </Alert>
        ) : null}

        {inviteUrl ? (
          <Stack gap="sm" align="center">
            <QRCodeSVG value={inviteUrl} size={200} level="M" />
            <Text size="xs" c="dimmed">
              This code expires in 7 days. Share it only with someone you trust.
            </Text>
            <Button variant="light" size="xs" onClick={() => setInviteUrl(null)}>
              Generate New Code
            </Button>
          </Stack>
        ) : (
          <Button
            onClick={handleGenerate}
            loading={createInviteMutation.isPending}
            leftSection={<IconQrcode size={16} />}
          >
            Generate QR Code
          </Button>
        )}
      </Stack>
    </Card>
  );
}
```

**Step 2: Export from components index**

Add to `web/src/features/trust/components/index.ts`:
```typescript
export { QRHandshakeGenerator } from './QRHandshake';
```

**Step 3: Run lint**

Run: `just lint-frontend`
Expected: PASS

**Step 4: Commit**

```bash
git add web/src/features/trust/components/QRHandshake.tsx web/src/features/trust/components/index.ts
git commit -m "feat(web): add QR handshake generation component"
```

### Task 4.3: QR scanner component

A camera-based scanner that reads QR codes and accepts the invite.

**Files:**
- Create: `web/src/features/trust/components/QRScanner.tsx`

**Step 1: Write the scanner component**

```typescript
// web/src/features/trust/components/QRScanner.tsx
import { useCallback, useEffect, useRef, useState } from 'react';
import { Html5Qrcode } from 'html5-qrcode';
import { Alert, Button, Card, Loader, Stack, Text, Title } from '@mantine/core';
import { IconAlertTriangle, IconCamera, IconCheck } from '@tabler/icons-react';
import { useAcceptInvite } from '../api';
import type { CryptoModule } from '@/providers/CryptoProvider';

interface QRScannerProps {
  deviceKid: string;
  privateKey: CryptoKey;
  wasmCrypto: CryptoModule;
}

export function QRScanner({ deviceKid, privateKey, wasmCrypto }: QRScannerProps) {
  const [scanning, setScanning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const scannerRef = useRef<Html5Qrcode | null>(null);
  const containerRef = useRef<string>('qr-scanner-container');
  const acceptMutation = useAcceptInvite(deviceKid, privateKey, wasmCrypto);

  const extractInviteId = useCallback((qrText: string): string | null => {
    // Expected format: https://host/handshake/{uuid}
    try {
      const url = new URL(qrText);
      const parts = url.pathname.split('/');
      const handshakeIdx = parts.indexOf('handshake');
      if (handshakeIdx >= 0 && parts[handshakeIdx + 1]) {
        return parts[handshakeIdx + 1];
      }
    } catch {
      // Not a URL — try treating as raw UUID
      const uuidRegex = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
      if (uuidRegex.test(qrText)) return qrText;
    }
    return null;
  }, []);

  const startScanning = useCallback(async () => {
    setError(null);
    setScanning(true);
    try {
      const scanner = new Html5Qrcode(containerRef.current);
      scannerRef.current = scanner;
      await scanner.start(
        { facingMode: 'environment' },
        { fps: 10, qrbox: { width: 250, height: 250 } },
        (decodedText) => {
          const inviteId = extractInviteId(decodedText);
          if (inviteId) {
            void scanner.stop().then(() => {
              scannerRef.current = null;
              setScanning(false);
              acceptMutation.mutate(inviteId);
            });
          }
        },
        () => {
          // QR not found in frame — ignore, keep scanning
        }
      );
    } catch (err) {
      setScanning(false);
      setError(err instanceof Error ? err.message : 'Camera access failed');
    }
  }, [extractInviteId, acceptMutation]);

  useEffect(() => {
    return () => {
      if (scannerRef.current) {
        void scannerRef.current.stop();
        scannerRef.current = null;
      }
    };
  }, []);

  return (
    <Card shadow="sm" padding="lg" radius="md" withBorder>
      <Stack gap="md">
        <Title order={4}>Scan QR Code</Title>

        <Text size="sm" c="dimmed">
          Scan another member's QR code to establish a trust connection.
        </Text>

        {error ? (
          <Alert icon={<IconAlertTriangle size={16} />} color="red">
            {error}
          </Alert>
        ) : null}

        {acceptMutation.isError ? (
          <Alert icon={<IconAlertTriangle size={16} />} color="red">
            {acceptMutation.error.message}
          </Alert>
        ) : null}

        {acceptMutation.isSuccess ? (
          <Alert icon={<IconCheck size={16} />} color="green">
            Handshake complete! Your trust score will update shortly.
          </Alert>
        ) : null}

        <div
          id={containerRef.current}
          style={{
            width: '100%',
            maxWidth: 300,
            margin: '0 auto',
            display: scanning ? 'block' : 'none',
          }}
        />

        {scanning ? (
          <Stack align="center" gap="xs">
            <Loader size="sm" />
            <Text size="sm" c="dimmed">Point your camera at a QR code...</Text>
            <Button
              variant="light"
              size="xs"
              onClick={() => {
                if (scannerRef.current) {
                  void scannerRef.current.stop();
                  scannerRef.current = null;
                }
                setScanning(false);
              }}
            >
              Cancel
            </Button>
          </Stack>
        ) : !acceptMutation.isSuccess ? (
          <Button
            onClick={() => void startScanning()}
            leftSection={<IconCamera size={16} />}
            loading={acceptMutation.isPending}
          >
            Start Scanning
          </Button>
        ) : null}
      </Stack>
    </Card>
  );
}
```

**Step 2: Export**

Add to `web/src/features/trust/components/index.ts`:
```typescript
export { QRScanner } from './QRScanner';
```

**Step 3: Run lint**

Run: `just lint-frontend`
Expected: PASS

**Step 4: Commit**

```bash
git add web/src/features/trust/components/QRScanner.tsx web/src/features/trust/components/index.ts
git commit -m "feat(web): add QR scanner component for handshake acceptance"
```

### Task 4.4: Handshake accept route

A route at `/handshake/:inviteId` that accepts when navigated directly (from scanning or clicking a link).

**Files:**
- Create: `web/src/pages/Handshake.page.tsx`
- Modify: `web/src/Router.tsx`

**Step 1: Create the Handshake page**

```typescript
// web/src/pages/Handshake.page.tsx
import { useEffect } from 'react';
import { Link } from '@tanstack/react-router';
import { Alert, Button, Card, Loader, Stack, Text, Title } from '@mantine/core';
import { IconAlertTriangle, IconCheck, IconHandshake } from '@tabler/icons-react';
import { useAcceptInvite } from '@/features/trust';
import { useCrypto } from '@/providers/CryptoProvider';
import { useDevice } from '@/providers/DeviceProvider';

interface HandshakePageProps {
  inviteId: string;
}

export function HandshakePage({ inviteId }: HandshakePageProps) {
  const { deviceKid, privateKey } = useDevice();
  const { crypto } = useCrypto();
  const acceptMutation = useAcceptInvite(deviceKid, privateKey, crypto);

  useEffect(() => {
    if (deviceKid && privateKey && crypto && !acceptMutation.isSuccess && !acceptMutation.isError && !acceptMutation.isPending) {
      acceptMutation.mutate(inviteId);
    }
    // Only run on mount
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [deviceKid, privateKey, crypto]);

  return (
    <Stack gap="md" maw={600} mx="auto" mt="xl" px="md">
      <Title order={2}>
        <IconHandshake size={28} style={{ verticalAlign: 'middle', marginRight: 8 }} />
        Trust Handshake
      </Title>

      <Card shadow="sm" padding="lg" radius="md" withBorder>
        {acceptMutation.isPending ? (
          <Stack align="center" gap="md">
            <Loader size="md" />
            <Text>Accepting handshake...</Text>
          </Stack>
        ) : null}

        {acceptMutation.isSuccess ? (
          <Stack gap="md">
            <Alert icon={<IconCheck size={16} />} color="green" title="Handshake Complete">
              You've been endorsed! Your trust score will update in a few moments.
            </Alert>
            <Button component={Link} to="/trust">
              View Trust Score
            </Button>
          </Stack>
        ) : null}

        {acceptMutation.isError ? (
          <Stack gap="md">
            <Alert icon={<IconAlertTriangle size={16} />} color="red" title="Handshake Failed">
              {acceptMutation.error.message}
            </Alert>
            <Button component={Link} to="/rooms" variant="light">
              Browse Rooms
            </Button>
          </Stack>
        ) : null}
      </Card>
    </Stack>
  );
}
```

**Step 2: Add route**

In `web/src/Router.tsx`:

```typescript
import { HandshakePage } from './pages/Handshake.page';

const handshakeRoute = createRoute({
  getParentRoute: () => authRequiredLayout,
  path: 'handshake/$inviteId',
  component: HandshakePageWrapper,
});

function HandshakePageWrapper() {
  const { inviteId } = useParams({ from: '/handshake/$inviteId' });
  return <HandshakePage inviteId={inviteId} />;
}
```

Add to route tree inside `authRequiredLayout.addChildren([ ... ])`.

**Step 3: Run lint**

Run: `just lint-frontend`
Expected: PASS

**Step 4: Commit**

```bash
git add web/src/pages/Handshake.page.tsx web/src/Router.tsx
git commit -m "feat(web): add handshake accept page and route"
```

### Task 4.5: Integrate QR components into Trust page

Add the QR generation and scanning cards to the trust dashboard.

**Files:**
- Modify: `web/src/pages/Trust.page.tsx`

**Step 1: Add QR components**

Import `QRHandshakeGenerator` and `QRScanner` from `@/features/trust`. Render them below the invites table when the user is authenticated and has a deviceKid + privateKey.

```typescript
{deviceKid && privateKey && crypto ? (
  <>
    <QRHandshakeGenerator deviceKid={deviceKid} privateKey={privateKey} wasmCrypto={crypto} />
    <QRScanner deviceKid={deviceKid} privateKey={privateKey} wasmCrypto={crypto} />
  </>
) : null}
```

**Step 2: Run lint**

Run: `just lint-frontend`
Expected: PASS

**Step 3: Commit**

```bash
git add web/src/pages/Trust.page.tsx
git commit -m "feat(web): add QR handshake components to trust dashboard"
```

### Task 4.6: QR handshake E2E test

Test the QR generation flow (camera scanning can't be tested in headless, but generation can).

**Files:**
- Modify: `web/tests/e2e/trust.spec.ts`

**Step 1: Add QR generation test**

```typescript
test('trust page shows QR handshake generator @smoke', async ({ page }) => {
  await signupUser(page);
  await page.goto('/trust');

  // QR Handshake section should be visible
  await expect(page.getByText(/QR Handshake/i)).toBeVisible({ timeout: 10_000 });

  // Generate button should be available
  const generateButton = page.getByRole('button', { name: /Generate QR Code/i });
  await expect(generateButton).toBeVisible();

  await test.info().attach('trust-qr-section', {
    body: await page.screenshot(),
    contentType: 'image/png',
  });
});

test('handshake page requires authentication', async ({ page }) => {
  await page.goto('/handshake/00000000-0000-0000-0000-000000000000');
  await expect(page.getByLabel(/username/i)).toBeVisible({ timeout: 5_000 });
  expect(page.url()).toContain('/login');
});
```

**Step 2: Run tests**

Run: `cd web && npx playwright test trust.spec.ts --project=chromium`
Expected: PASS

**Step 3: Commit**

```bash
git add web/tests/e2e/trust.spec.ts
git commit -m "test(e2e): add QR handshake and handshake page smoke tests"
```

---

## Milestone Summary

| Milestone | Tasks | What it delivers |
|-----------|-------|-----------------|
| M1 | 1.1–1.4 | Confidence that trust migration didn't break existing flows |
| M2 | 2.1–2.4 | Regression safety net covering full user journey |
| M3 | 3.1–3.7 | Trust-aware UI: scores, budget, invites, tier badges |
| M4 | 4.1–4.6 | QR handshake flow: generate, scan, accept, with E2E tests |

## Review Checkpoints

After each milestone, run:
```bash
just lint && just test-frontend
cd web && npx playwright test --project=chromium
```

Before opening a PR, run:
```bash
just lint && just test
```

## Dependencies & Sequencing

```
M1 (contract validation) → can start immediately
M2 (smoke tests) → can start in parallel with M1
M3 (trust UI) → depends on M1 findings (Room type shape)
M4 (QR handshake) → depends on M3 (trust API client and hooks)
```

M1 and M2 are parallelizable. M3 and M4 are sequential.

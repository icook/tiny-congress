# Frontend Identity Implementation Plan (TanStack Query)

## Overview

The `feature/phase1-identity-tickets` branch contains backend identity endpoints but no frontend implementation. Master was rebased with TanStack Query setup. This plan implements FE-01 through FE-07 using TanStack Query patterns.

## Directory Structure

Following `docs/interfaces/directory-conventions.md`:

```
web/src/features/identity/
├── api/
│   ├── client.ts           # REST client + types
│   └── queries.ts          # TanStack Query factories
├── keys/
│   ├── index.ts            # Barrel exports
│   ├── types.ts            # Key storage types
│   ├── storage.ts          # IndexedDB via idb-keyval
│   ├── crypto.ts           # Ed25519 signing (@noble/curves)
│   └── canonical.ts        # RFC 8785 JSON canonicalization
├── screens/
│   ├── Signup.tsx          # FE-02
│   ├── Signup.test.tsx
│   ├── Login.tsx           # FE-03
│   ├── Login.test.tsx
│   ├── Devices.tsx         # FE-04
│   ├── Devices.test.tsx
│   ├── Profile.tsx         # FE-05
│   ├── Profile.test.tsx
│   ├── Endorsements.tsx    # FE-06
│   ├── Endorsements.test.tsx
│   ├── Recovery.tsx        # FE-07
│   └── Recovery.test.tsx
├── components/
│   ├── EndorsementEditor.tsx
│   └── EndorsementEditor.test.tsx
└── state/
    └── session.ts          # Session context (account_id, device_id)
```

## API Types (from Rust handlers)

```typescript
// web/src/features/identity/api/client.ts

// === Auth ===
interface SignupRequest {
  username: string;
  root_pubkey: string;           // base64url
  device_pubkey: string;         // base64url
  device_metadata?: { name?: string; type?: string };
  delegation_envelope: SignedEnvelope;
}

interface SignupResponse {
  account_id: string;            // UUID
  device_id: string;             // UUID
  root_kid: string;
}

interface ChallengeRequest {
  account_id: string;
  device_id: string;
}

interface ChallengeResponse {
  challenge_id: string;
  nonce: string;                 // base64url
  expires_at: string;            // ISO datetime
}

interface VerifyRequest {
  challenge_id: string;
  account_id: string;
  device_id: string;
  signature: string;             // base64url
}

interface VerifyResponse {
  session_id: string;
  expires_at: string;
}

// === Devices ===
interface AddDeviceRequest {
  account_id: string;
  device_pubkey: string;
  device_metadata?: { name?: string; type?: string };
  delegation_envelope: SignedEnvelope;
}

interface AddDeviceResponse {
  device_id: string;
  device_kid: string;
}

interface RevokeDeviceRequest {
  account_id: string;
  delegation_envelope: SignedEnvelope;  // Contains device_id in payload
}

// === Endorsements ===
interface EndorsementCreateRequest {
  account_id: string;
  device_id: string;
  envelope: SignedEnvelope;
}

interface EndorsementCreateResponse {
  endorsement_id: string;
}

interface EndorsementRevokeRequest {
  account_id: string;
  device_id: string;
  envelope: SignedEnvelope;
}

// === Recovery ===
interface RecoveryPolicyRequest {
  account_id: string;
  envelope: SignedEnvelope;
}

interface RecoveryPolicyResponse {
  policy_id: string;
  threshold: number;
  helpers: Array<{ helper_account_id: string; helper_root_kid?: string }>;
}

interface RecoveryApprovalRequest {
  account_id: string;
  helper_account_id: string;
  helper_device_id: string;
  policy_id: string;
  envelope: SignedEnvelope;
}

interface RootRotationRequest {
  account_id: string;
  envelope: SignedEnvelope;
}

// === Shared ===
interface SignedEnvelope {
  payload: Record<string, unknown>;
  signer: {
    kid: string;
    account_id?: string;
    device_id?: string;
  };
  signature: string;             // base64url
}
```

## TanStack Query Factories

Following the pattern from `web/src/api/queries.ts`:

```typescript
// web/src/features/identity/api/queries.ts

import { queryOptions, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  getRecoveryPolicy,
  listEndorsements,
  listDevices,
  // ... API functions
} from './client';

// === Query Keys ===
export const identityKeys = {
  all: ['identity'] as const,
  session: () => [...identityKeys.all, 'session'] as const,
  devices: (accountId: string) => [...identityKeys.all, 'devices', accountId] as const,
  endorsements: (accountId: string) => [...identityKeys.all, 'endorsements', accountId] as const,
  recoveryPolicy: (accountId: string) => [...identityKeys.all, 'recovery', accountId] as const,
  profile: (accountId: string) => [...identityKeys.all, 'profile', accountId] as const,
};

// === Queries ===
export const devicesQuery = (accountId: string) => queryOptions({
  queryKey: identityKeys.devices(accountId),
  queryFn: () => listDevices(accountId),
  staleTime: 60 * 1000, // 1 minute
});

export const endorsementsQuery = (accountId: string, topic?: string) => queryOptions({
  queryKey: [...identityKeys.endorsements(accountId), topic],
  queryFn: () => listEndorsements(accountId, topic),
  staleTime: 30 * 1000, // 30 seconds
});

export const recoveryPolicyQuery = (accountId: string) => queryOptions({
  queryKey: identityKeys.recoveryPolicy(accountId),
  queryFn: () => getRecoveryPolicy(accountId),
  staleTime: 5 * 60 * 1000, // 5 minutes
});

// === Mutation Hooks ===
export function useSignup() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: signup,
    onSuccess: (data) => {
      // Store session data, invalidate queries
      queryClient.invalidateQueries({ queryKey: identityKeys.all });
    },
  });
}

export function useLogin() {
  // Challenge + verify flow
}

export function useAddDevice() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: addDevice,
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({
        queryKey: identityKeys.devices(variables.account_id)
      });
    },
  });
}

export function useRevokeDevice() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: revokeDevice,
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({
        queryKey: identityKeys.devices(variables.account_id)
      });
    },
  });
}

export function useCreateEndorsement() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: createEndorsement,
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({
        queryKey: identityKeys.endorsements(variables.account_id)
      });
    },
  });
}

export function useRevokeEndorsement() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: revokeEndorsement,
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({
        queryKey: identityKeys.endorsements(variables.account_id)
      });
    },
  });
}

export function useSetRecoveryPolicy() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: setRecoveryPolicy,
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({
        queryKey: identityKeys.recoveryPolicy(variables.account_id)
      });
    },
  });
}
```

## Key Management Module (FE-01)

Dependencies to add:
```json
{
  "@noble/curves": "^1.4.0",
  "idb-keyval": "^6.2.1"
}
```

```typescript
// web/src/features/identity/keys/types.ts
export interface StoredKey {
  kid: string;                    // Key ID (SHA-256 of pubkey, truncated)
  publicKey: string;              // base64url encoded
  privateKey: string;             // base64url encoded (encrypted at rest ideally)
  createdAt: string;
  label?: string;
}

export interface KeyPair {
  publicKey: Uint8Array;
  privateKey: Uint8Array;
  kid: string;
}

// web/src/features/identity/keys/crypto.ts
import { ed25519 } from '@noble/curves/ed25519';
import { sha256 } from '@noble/hashes/sha256';

export function generateKeyPair(): KeyPair { /* ... */ }
export function sign(message: Uint8Array, privateKey: Uint8Array): Uint8Array { /* ... */ }
export function deriveKid(publicKey: Uint8Array): string { /* ... */ }

// web/src/features/identity/keys/canonical.ts
// RFC 8785 JSON Canonicalization Scheme
export function canonicalize(value: unknown): string { /* ... */ }

// web/src/features/identity/keys/storage.ts
import { get, set, del } from 'idb-keyval';

export async function storeRootKey(key: StoredKey): Promise<void> { /* ... */ }
export async function storeDeviceKey(key: StoredKey): Promise<void> { /* ... */ }
export async function getRootKey(): Promise<StoredKey | undefined> { /* ... */ }
export async function getDeviceKey(): Promise<StoredKey | undefined> { /* ... */ }
```

## Session State

```typescript
// web/src/features/identity/state/session.ts
import { createContext, useContext, useState, useEffect, ReactNode } from 'react';

interface Session {
  accountId: string;
  deviceId: string;
  sessionId: string;
  expiresAt: Date;
}

interface SessionContextValue {
  session: Session | null;
  setSession: (session: Session | null) => void;
  isAuthenticated: boolean;
  logout: () => void;
}

const SessionContext = createContext<SessionContextValue | null>(null);

export function SessionProvider({ children }: { children: ReactNode }) {
  // Load from localStorage on mount, sync with TanStack Query
}

export function useSession() {
  const context = useContext(SessionContext);
  if (!context) throw new Error('useSession must be within SessionProvider');
  return context;
}
```

## Router Updates

```typescript
// web/src/Router.tsx - additions
import { SignupPage } from './features/identity/screens/Signup';
import { LoginPage } from './features/identity/screens/Login';
import { DevicesPage } from './features/identity/screens/Devices';
import { ProfilePage } from './features/identity/screens/Profile';
import { EndorsementsPage } from './features/identity/screens/Endorsements';
import { RecoveryPage } from './features/identity/screens/Recovery';

// Replace placeholder routes:
const signupRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: 'signup',
  component: SignupPage,
});

const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: 'login',
  component: LoginPage,
});

// Protected routes (require session):
const accountRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: 'account',
  component: ProfilePage,
});

const devicesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: 'devices',
  component: DevicesPage,
});

const endorsementsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: 'endorsements',
  component: EndorsementsPage,
});

const recoveryRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: 'recovery',
  component: RecoveryPage,
});
```

## Implementation Order

1. **FE-01: Key Management** (foundation)
   - Add dependencies (@noble/curves, idb-keyval)
   - Implement crypto.ts, canonical.ts, storage.ts
   - Unit tests for signing/verification

2. **FE-02: Signup Flow**
   - Create API client functions
   - Create SignupPage with form
   - Generate root + device keys on submit
   - Create delegation envelope
   - Call signup mutation
   - Store keys + session

3. **FE-03: Login Flow**
   - Load device key from storage
   - Issue challenge mutation
   - Sign challenge with device key
   - Verify challenge mutation
   - Store session

4. **FE-04: Device Management**
   - DevicesPage with useQuery for device list
   - Add device form + mutation
   - Revoke device action + mutation

5. **FE-05: Profile Page**
   - Display account info, tier badge
   - Show reputation score
   - List endorsements by topic

6. **FE-06: Endorsement Editor**
   - EndorsementEditor component (magnitude/confidence sliders)
   - Create endorsement mutation
   - Revoke endorsement mutation

7. **FE-07: Recovery Setup**
   - Recovery policy query
   - Set policy mutation
   - Approve recovery flow
   - Root rotation mutation

## Testing Strategy

Each screen should have:
1. Render test (loading state)
2. Success state test (with mocked query data)
3. Error state test
4. Mutation tests (verify API calls)

Mock setup in test files:
```typescript
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render } from '../../../test-utils';

// Mock the API client
vi.mock('../api/client', () => ({
  signup: vi.fn(),
  // ...
}));
```

## Notes

- All API calls to `/api/identity/*` routes
- Backend base URL configured via environment variable
- Keys stored in IndexedDB (not localStorage for security)
- Session token in memory + localStorage for persistence
- Protected routes redirect to /login when no session

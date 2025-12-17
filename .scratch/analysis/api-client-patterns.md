# API Client Pattern Analysis

## Current State

The identity feature uses a manual fetch pattern in `web/src/features/identity/api/client.ts`:

```typescript
async function fetchJson<T>(endpoint: string, options?: RequestInit): Promise<T> {
  const response = await fetch(url, { ...options, headers });
  if (!response.ok) throw new ApiError(...);
  return body as T;
}

export async function getProfile(accountId: string): Promise<Profile> {
  return fetchJson<Profile>(`/users/${accountId}`);
}
```

### Usage Pattern (Profile.tsx)

```typescript
const [profile, setProfile] = useState<ProfileData | null>(null);
const [loading, setLoading] = useState(true);
const [error, setError] = useState<string | null>(null);

useEffect(() => {
  const fetch = async () => {
    try {
      const [posture, endorsements, reputation] = await Promise.allSettled([
        getSecurityPosture(token, accountId),
        getEndorsements(accountId),
        getReputationScore(accountId),
      ]);
      // ... process results
    } catch (err) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  };
  fetch();
}, []);
```

## Problems

| Issue | Impact |
|-------|--------|
| **No caching** | Same data fetched multiple times across pages |
| **Manual state** | 3 useState calls per query (data, loading, error) |
| **No background refetch** | Data goes stale, user must manually refresh |
| **Race conditions** | useEffect cleanup not handled |
| **No deduplication** | Concurrent requests for same resource |
| **Complex parallel fetching** | Promise.allSettled boilerplate everywhere |
| **No optimistic updates** | Mutations feel slow |

## TanStack Query (React Query v5) Solution

### Benefits

1. **Automatic caching** - Data shared across components
2. **Background refetching** - Stale-while-revalidate pattern
3. **Deduplication** - Concurrent requests coalesced
4. **Declarative** - No manual loading/error state
5. **Devtools** - Cache inspection, query invalidation
6. **Mutations** - Optimistic updates, automatic invalidation

### Proposed Pattern

```typescript
// api/queries.ts
import { queryOptions, useMutation, useQueryClient } from '@tanstack/react-query';
import * as api from './client';

export const profileQueries = {
  detail: (accountId: string) => queryOptions({
    queryKey: ['profile', accountId],
    queryFn: () => api.getProfile(accountId),
  }),

  securityPosture: (token: string, accountId: string) => queryOptions({
    queryKey: ['profile', accountId, 'security-posture'],
    queryFn: () => api.getSecurityPosture(token, accountId),
  }),

  endorsements: (accountId: string) => queryOptions({
    queryKey: ['profile', accountId, 'endorsements'],
    queryFn: () => api.getEndorsements(accountId),
  }),
};

export function useCreateEndorsement() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: api.createEndorsement,
    onSuccess: (_, variables) => {
      // Invalidate relevant queries
      queryClient.invalidateQueries({
        queryKey: ['profile', variables.subjectId, 'endorsements']
      });
    },
  });
}
```

### Component Usage

```typescript
// Before: ~40 lines of state management
// After: ~10 lines

function Profile() {
  const session = getSession();

  const { data: posture, isPending: postureLoading } = useQuery(
    profileQueries.securityPosture(session.token, session.accountId)
  );

  const { data: endorsements, isPending: endorsementsLoading } = useQuery(
    profileQueries.endorsements(session.accountId)
  );

  // Parallel queries with useSuspenseQueries for cleaner loading states
  // Or useQueries for non-suspense parallel fetching

  if (postureLoading || endorsementsLoading) return <Skeleton />;

  return <ProfileView posture={posture} endorsements={endorsements} />;
}
```

### Query Key Convention

```
['entity', entityId, 'sub-resource']
['profile', '123', 'endorsements']
['devices']
['recovery-policy', accountId]
```

## Migration Path

### Phase 1: Setup
1. Add `@tanstack/react-query` dependency
2. Add `QueryClientProvider` to app root
3. Configure defaults (staleTime, gcTime)

### Phase 2: Query Layer
1. Create `api/queries.ts` with query options factories
2. Create `api/mutations.ts` for mutations with invalidation

### Phase 3: Migrate Screens
1. Replace useState/useEffect with useQuery
2. Replace mutation handlers with useMutation
3. Add loading/error boundaries

### Phase 4: Enhancements
1. Add React Query Devtools
2. Implement optimistic updates for mutations
3. Consider Suspense boundaries

## Stale Time Recommendations

| Query Type | staleTime | Rationale |
|------------|-----------|-----------|
| Profile | 5 min | Rarely changes |
| Devices | 1 min | Security-sensitive |
| Endorsements | 30 sec | Social data, more dynamic |
| Session | Infinity | Only changes on login/logout |

## File Structure

```
web/src/features/identity/api/
├── client.ts       # Raw fetch functions (keep)
├── queries.ts      # Query options factories (new)
├── mutations.ts    # Mutation hooks (new)
└── keys.ts         # Query key factories (optional)
```

## Decision Needed

Should this be implemented as part of the identity feature work, or deferred to a separate infrastructure ticket?

Recommendation: **Separate ticket** - This is cross-cutting infrastructure that will benefit all features. Identity feature can continue with current pattern, then migrate when query layer is ready.

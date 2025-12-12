# TanStack Query Migration Plan

## Why Migrate

### Current Pattern Problems

The identity feature demonstrates the pain points of manual data fetching:

**1. State Management Boilerplate**

Every component that fetches data requires:
```typescript
const [data, setData] = useState<T | null>(null);
const [loading, setLoading] = useState(true);
const [error, setError] = useState<string | null>(null);

useEffect(() => {
  const fetchData = async () => {
    try {
      setLoading(true);
      const result = await api.getData();
      setData(result);
    } catch (err) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  };
  fetchData();
}, [dependencies]);
```

This is ~15 lines repeated in every data-fetching component.

**2. No Caching**

- Navigate to Profile → fetch endorsements
- Navigate to Devices → fetch devices
- Navigate back to Profile → fetch endorsements again

Same data fetched repeatedly. Users wait for loading spinners on every navigation.

**3. No Background Refetching**

Data fetched once and never updated. If another tab creates an endorsement, the Profile page shows stale data until manual refresh.

**4. Race Conditions**

```typescript
useEffect(() => {
  fetchData(); // What if component unmounts mid-fetch?
}, [id]);      // What if id changes before fetch completes?
```

No cleanup, no cancellation. Can lead to setting state on unmounted components or showing data for wrong entity.

**5. Parallel Fetch Complexity**

Profile.tsx uses `Promise.allSettled` to fetch 4 resources in parallel:
```typescript
const [posture, endorsements, reputation, devices] = await Promise.allSettled([
  getSecurityPosture(token, accountId).catch(() => null),
  getEndorsements(accountId).catch(() => [[], null]),
  getReputationScore(accountId).catch(() => null),
  listDevices(token).catch(() => []),
]);

// Then process each result individually...
if (postureResult.status === 'fulfilled' && postureResult.value) {
  setPosture(postureResult.value);
} else { /* fallback logic */ }
```

40+ lines just for data fetching orchestration.

**6. No Mutation Coordination**

After creating an endorsement, must manually refetch affected queries:
```typescript
const handleSubmit = async () => {
  await createEndorsement(data);
  // Must manually trigger refetch - easy to forget
  await fetchEndorsements();
};
```

No optimistic updates. UI feels slow.

---

## What TanStack Query Provides

### Automatic Caching

```typescript
// Component A
const { data } = useQuery({ queryKey: ['profile', id], queryFn: fetchProfile });

// Component B (different part of app)
const { data } = useQuery({ queryKey: ['profile', id], queryFn: fetchProfile });
// ^ Instant! Uses cached data, no network request
```

### Stale-While-Revalidate

Shows cached data immediately, refetches in background:
- `staleTime`: How long data is "fresh" (no refetch)
- `gcTime`: How long to keep unused data in cache

```typescript
useQuery({
  queryKey: ['profile', id],
  queryFn: fetchProfile,
  staleTime: 5 * 60 * 1000, // Fresh for 5 minutes
});
```

### Automatic Refetching

Configurable triggers:
- Window focus (user returns to tab)
- Network reconnect
- Interval polling
- Manual invalidation

### Request Deduplication

Multiple components requesting same data simultaneously → single network request.

### Declarative Loading/Error States

```typescript
const { data, isPending, isError, error } = useQuery({
  queryKey: ['profile', id],
  queryFn: fetchProfile,
});

if (isPending) return <Skeleton />;
if (isError) return <Error message={error.message} />;
return <Profile data={data} />;
```

### Mutation + Cache Invalidation

```typescript
const mutation = useMutation({
  mutationFn: createEndorsement,
  onSuccess: () => {
    // Automatically refetch affected queries
    queryClient.invalidateQueries({ queryKey: ['endorsements'] });
  },
});
```

### Optimistic Updates

```typescript
const mutation = useMutation({
  mutationFn: createEndorsement,
  onMutate: async (newEndorsement) => {
    // Cancel outgoing refetches
    await queryClient.cancelQueries({ queryKey: ['endorsements'] });

    // Snapshot previous value
    const previous = queryClient.getQueryData(['endorsements']);

    // Optimistically update
    queryClient.setQueryData(['endorsements'], old => [...old, newEndorsement]);

    return { previous };
  },
  onError: (err, variables, context) => {
    // Rollback on error
    queryClient.setQueryData(['endorsements'], context.previous);
  },
});
```

### DevTools

Visual inspection of:
- All cached queries
- Query states (fresh, stale, fetching)
- Manual invalidation/refetch triggers
- Cache timing

---

## Migration Plan

### Phase 0: Trailblazer (feature/000-build-info-about)

Use a simple feature to establish patterns before tackling identity.

**Goals:**
- Add TanStack Query dependency
- Set up QueryClientProvider
- Establish query/mutation file conventions
- Validate patterns work with existing infrastructure

**Deliverables:**
1. `@tanstack/react-query` + `@tanstack/react-query-devtools` added
2. `web/src/providers/QueryProvider.tsx` - client configuration
3. `web/src/App.tsx` - wrap with provider
4. Simple query example in build-info feature

### Phase 1: Infrastructure Setup

**Query Client Configuration:**

```typescript
// src/providers/QueryProvider.tsx
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ReactQueryDevtools } from '@tanstack/react-query-devtools';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 60 * 1000,        // 1 minute default
      gcTime: 5 * 60 * 1000,       // 5 minutes
      retry: 1,                     // Retry once on failure
      refetchOnWindowFocus: true,
    },
  },
});

export function QueryProvider({ children }) {
  return (
    <QueryClientProvider client={queryClient}>
      {children}
      <ReactQueryDevtools initialIsOpen={false} />
    </QueryClientProvider>
  );
}
```

### Phase 2: Query Layer Pattern

**File Structure:**

```
web/src/features/{feature}/api/
├── client.ts       # Raw fetch functions (unchanged)
├── queries.ts      # Query option factories
└── mutations.ts    # Mutation hooks
```

**Query Options Factory Pattern:**

```typescript
// features/identity/api/queries.ts
import { queryOptions } from '@tanstack/react-query';
import * as api from './client';

export const identityQueries = {
  // Devices
  devices: (token: string) => queryOptions({
    queryKey: ['devices'],
    queryFn: () => api.listDevices(token),
    staleTime: 60 * 1000, // 1 minute - security sensitive
  }),

  // Profile
  securityPosture: (token: string, accountId: string) => queryOptions({
    queryKey: ['profile', accountId, 'security-posture'],
    queryFn: () => api.getSecurityPosture(token, accountId),
    staleTime: 5 * 60 * 1000, // 5 minutes
  }),

  endorsements: (accountId: string) => queryOptions({
    queryKey: ['profile', accountId, 'endorsements'],
    queryFn: () => api.getEndorsements(accountId),
    staleTime: 30 * 1000, // 30 seconds - more dynamic
  }),

  reputation: (accountId: string) => queryOptions({
    queryKey: ['profile', accountId, 'reputation'],
    queryFn: () => api.getReputationScore(accountId),
    staleTime: 5 * 60 * 1000,
  }),

  // Recovery
  recoveryPolicy: (accountId: string) => queryOptions({
    queryKey: ['recovery-policy', accountId],
    queryFn: () => api.getRecoveryPolicy(accountId),
  }),
};
```

**Mutation Hooks:**

```typescript
// features/identity/api/mutations.ts
import { useMutation, useQueryClient } from '@tanstack/react-query';
import * as api from './client';

export function useCreateEndorsement() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (params: { token: string; request: api.CreateEndorsementRequest }) =>
      api.createEndorsement(params.token, params.request),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({
        queryKey: ['profile', variables.request.subject_id, 'endorsements'],
      });
    },
  });
}

export function useRevokeDevice() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (params: { token: string; deviceId: string; request: api.RevokeDeviceRequest }) =>
      api.revokeDevice(params.token, params.deviceId, params.request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['devices'] });
    },
  });
}
```

### Phase 3: Migrate Identity Screens

**Priority Order:**
1. `Devices.tsx` - Simple list, good starting point
2. `Profile.tsx` - Complex, multiple parallel queries
3. `Endorsements.tsx` - Mutations + cache invalidation
4. `Recovery.tsx` - Policy management

**Before (Devices.tsx):**
```typescript
const [devices, setDevices] = useState<Device[]>([]);
const [loading, setLoading] = useState(true);
const [error, setError] = useState<string | null>(null);

useEffect(() => {
  const fetchDevices = async () => {
    try {
      const result = await listDevices(token);
      setDevices(result);
    } catch (err) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  };
  fetchDevices();
}, [token]);
```

**After:**
```typescript
const { data: devices, isPending, isError, error } = useQuery(
  identityQueries.devices(token)
);

if (isPending) return <DevicesSkeleton />;
if (isError) return <ErrorAlert message={error.message} />;
return <DevicesList devices={devices} />;
```

### Phase 4: Advanced Patterns

**Parallel Queries (Profile.tsx):**

```typescript
const results = useQueries({
  queries: [
    identityQueries.securityPosture(token, accountId),
    identityQueries.endorsements(accountId),
    identityQueries.reputation(accountId),
  ],
});

const [posture, endorsements, reputation] = results;
const isLoading = results.some(r => r.isPending);
```

**Dependent Queries:**

```typescript
const { data: session } = useQuery(sessionQuery);

const { data: profile } = useQuery({
  ...identityQueries.securityPosture(session?.token, session?.accountId),
  enabled: !!session, // Only fetch when session exists
});
```

**Optimistic Device Revocation:**

```typescript
const revokeMutation = useMutation({
  mutationFn: revokeDevice,
  onMutate: async ({ deviceId }) => {
    await queryClient.cancelQueries({ queryKey: ['devices'] });
    const previous = queryClient.getQueryData(['devices']);

    queryClient.setQueryData(['devices'], (old: Device[]) =>
      old.map(d => d.device_id === deviceId
        ? { ...d, revoked_at: new Date().toISOString() }
        : d
      )
    );

    return { previous };
  },
  onError: (_, __, context) => {
    queryClient.setQueryData(['devices'], context?.previous);
  },
  onSettled: () => {
    queryClient.invalidateQueries({ queryKey: ['devices'] });
  },
});
```

---

## Query Key Conventions

```
['entity']                           # List
['entity', id]                       # Detail
['entity', id, 'sub-resource']       # Nested resource
['entity', id, 'sub-resource', subId] # Nested detail
```

**Examples:**
```typescript
['devices']                          // All devices
['profile', '123']                   // Profile for account 123
['profile', '123', 'endorsements']   // Endorsements for account 123
['profile', '123', 'security-posture']
['recovery-policy', '123']
```

**Invalidation cascades:**
```typescript
// Invalidate all profile-related queries for account 123
queryClient.invalidateQueries({ queryKey: ['profile', '123'] });

// Invalidate only endorsements
queryClient.invalidateQueries({ queryKey: ['profile', '123', 'endorsements'] });
```

---

## Stale Time Recommendations

| Query Type | staleTime | Rationale |
|------------|-----------|-----------|
| Build info | 1 hour | Rarely changes |
| Devices | 1 min | Security-sensitive, want fresh data |
| Recovery policy | 5 min | Rarely changes after setup |
| Profile/posture | 5 min | Stable user data |
| Endorsements | 30 sec | Social data, more dynamic |
| Reputation | 5 min | Computed aggregate, stable |

---

## Testing Strategy

**Mock at Query Level:**

```typescript
// Use QueryClient wrapper in tests
const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false } },
});

function renderWithQuery(ui: React.ReactElement) {
  return render(
    <QueryClientProvider client={queryClient}>
      {ui}
    </QueryClientProvider>
  );
}

// Pre-populate cache for tests
queryClient.setQueryData(['devices'], mockDevices);
```

---

## Rollout Sequence

1. **feature/000-build-info-about** - Add TanStack Query, establish patterns
2. **Merge to master** - Infrastructure available repo-wide
3. **feature/tanstack-identity-migration** - Migrate identity feature
4. **Future features** - Use TanStack Query from the start

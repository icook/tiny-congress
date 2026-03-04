# ADR-013: Frontend Architecture

## Status
Accepted

## Context

TinyCongress's frontend consumes two API surfaces (GraphQL and REST, per [ADR-012](012-dual-api-surface.md)) and performs client-side cryptographic operations via a WASM module (per [ADR-006](006-wasm-crypto-sharing.md)). The frontend must work with a single Docker image across all environments and generate TypeScript types from both API specs to prevent type drift.

Several tensions shaped this decision:

- **Type safety vs. bundle size.** Full-featured GraphQL clients (Apollo, urql) add 30–50KB+ and bring their own caching, normalization, and state management. TinyCongress needs type-safe API calls, not a GraphQL runtime.
- **Build-time vs. runtime configuration.** Vite bakes `import.meta.env` values into the bundle at build time. A single image serving multiple environments needs runtime configuration injection.
- **Sync vs. async WASM loading.** WASM modules load asynchronously. Components that need cryptographic operations must either block rendering until WASM is ready or handle a loading state on every crypto call.

## Decision

### TanStack Query for server state

TanStack Query (React Query v5) manages all server state with conservative defaults:

```typescript
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 60 * 1000,       // 1 minute
      gcTime: 5 * 60 * 1000,      // 5 minutes
      retry: 1,                    // Single retry
      refetchOnWindowFocus: true,
    },
  },
});
```

Query-specific overrides are possible (e.g., `buildInfoQuery` uses 1-hour staleTime and infinite gcTime since build info rarely changes).

React Query Devtools are included in development builds (initialized closed).

### Hand-written fetch wrapper

A ~40-line `graphqlClient.ts` handles GraphQL requests:

```typescript
export async function graphqlRequest<TData>(
  query: string,
  variables?: Record<string, unknown>,
): Promise<TData> {
  // POST to getGraphqlUrl(), check response.ok, validate errors array,
  // ensure data property exists, return typed data
}
```

A separate ~64-line `fetchJson<T>()` utility in `features/identity/api/client.ts` handles REST calls with header merging and error parsing.

No retries, caching, or normalization — those are TanStack Query's responsibility. The fetch wrappers only handle request construction, response parsing, and error surfacing.

### Dual codegen: GraphQL types + Zod schemas, REST types

Two codegen pipelines produce TypeScript types from the backend specs:

**GraphQL codegen** (`web/codegen.ts`):
- Input: `web/schema.graphql` (exported from Rust via `export_schema` binary)
- Output: `web/src/api/generated/graphql.ts`
- Plugins: `@graphql-codegen/typescript` for type definitions, `graphql-codegen-typescript-validation-schema` with `zodv4` schema mode
- Produces both TypeScript interfaces and Zod v4 validation schemas for runtime type checking

Generated Zod schemas example:
```typescript
export const BuildInfoSchema: z.ZodObject<Properties<BuildInfo>> = z.object({
    buildTime: z.string(),
    gitSha: z.string(),
    message: z.string().nullish(),
    version: z.string(),
});
```

**OpenAPI codegen**:
- Tool: `openapi-typescript` v7
- Input: `web/openapi.json` (exported from Rust via `export_openapi` binary)
- Output: `web/src/api/generated/rest.ts`
- Produces type-safe interfaces for REST paths, request bodies, and response schemas (including `ProblemDetails` and `ProblemExtensions`)

**Freshness enforcement:** CI runs the `codegen-check` job which re-generates both outputs and fails if the committed files differ from the generated output. This prevents stale types from reaching production.

### `queryOptions` pattern for cache key management

Query configuration is centralized using TanStack Query's `queryOptions` helper:

```typescript
export const buildInfoQuery = queryOptions({
  queryKey: ['build-info'],
  queryFn: fetchBuildInfo,
  staleTime: 60 * 60 * 1000,  // 1 hour
  gcTime: Infinity,
});

// Usage in components:
const { data } = useQuery(buildInfoQuery);
```

This co-locates cache keys with query functions, preventing key typos and enabling type-safe invalidation. Components import the query options object rather than assembling keys and functions independently.

### Runtime config via `window.__TC_ENV__`

A two-tier configuration strategy enables one Docker image for all environments:

```typescript
interface RuntimeConfig {
  VITE_API_URL?: string;
  TC_ENVIRONMENT?: string;
}

declare global {
  interface Window {
    __TC_ENV__?: RuntimeConfig;
  }
}
```

Resolution order:
1. `window.__TC_ENV__` — set by `/config.js` at container startup (see [ADR-014](014-ci-pipeline.md) for the injection mechanism)
2. `import.meta.env.VITE_*` — Vite build-time values (for local dev without Docker)
3. Hardcoded fallback: `'http://localhost:8080'`

`getApiBaseUrl()` and `getEnvironment()` are the only access points — components never read `window.__TC_ENV__` directly.

### CryptoProvider for async WASM loading

The `CryptoProvider` component (~129 lines) handles async WASM module initialization:

```typescript
export interface CryptoModule {
  derive_kid: (publicKey: Uint8Array) => string;
  encode_base64url: (bytes: Uint8Array) => string;
  decode_base64url: (encoded: string) => Uint8Array;
}
```

Loading strategy:
1. Dynamic import: `import('@/wasm/tc-crypto/tc_crypto.js')`
2. Async WASM initialization via `wasm.default()`
3. Code-split — WASM loads separately from the main JavaScript bundle

The provider returns `null` (blocks child rendering) until WASM is initialized. Two hooks expose the module:
- `useCrypto()` — returns `{ crypto, isLoading, error }` for components that handle loading states
- `useCryptoRequired()` — returns the module directly, throws if not loaded (safe to use inside the provider tree)

This blocking approach ensures that any component inside the provider tree can safely call `useCryptoRequired()` without null checks.

### Bundle code splitting

Vite's `manualChunks` configuration splits the bundle into predictable chunks:

```javascript
manualChunks: {
  'react-vendor': ['react', 'react-dom'],
  'router': ['@tanstack/react-router'],
  'query': ['@tanstack/react-query'],
  'mantine': ['@mantine/core', '@mantine/hooks'],
  'icons': ['@tabler/icons-react'],
}
```

The WASM module is loaded via dynamic import and automatically code-split.

## Consequences

### Positive
- The fetch wrapper is ~40 lines with no runtime dependencies beyond `fetch`. No GraphQL client library to update, configure, or debug.
- Dual codegen catches type drift at CI time — a backend schema change that isn't regenerated fails the `codegen-check` job.
- Zod schemas enable runtime validation of API responses, catching backend/frontend contract violations during development.
- `window.__TC_ENV__` lets one Docker image serve any environment. No rebuild needed for staging vs. production.
- `CryptoProvider` eliminates null checks throughout the component tree — WASM is guaranteed available inside the provider.
- `queryOptions` pattern prevents cache key mismatches and enables type-safe query invalidation.

### Negative
- Two codegen pipelines (GraphQL + OpenAPI) add build complexity and a CI freshness check. Developers must run `just codegen` after backend schema changes.
- The hand-written fetch wrapper lacks features that mature GraphQL clients provide (automatic cache normalization, optimistic updates, subscription support). These must be built manually if needed.
- `CryptoProvider` blocks rendering of the entire subtree during WASM load (~200KB). On slow connections, this creates a visible loading delay.
- `window.__TC_ENV__` is a global mutable — it's not type-safe beyond the TypeScript declaration, and any script on the page could modify it.

### Neutral
- React Query Devtools are included in dev builds but tree-shaken from production. No bundle size impact in production.
- The `config.js` script loaded by `index.html` is served with `Cache-Control: no-store` to ensure environment changes take effect immediately without cache invalidation.
- Coverage excludes WASM-related files (`src/wasm/**`, `CryptoProvider.tsx`) since they depend on WASM initialization that's impractical in unit tests.

## Alternatives considered

### Apollo Client or urql for GraphQL
- Full-featured: normalized cache, subscriptions, devtools
- Rejected for bundle size (30–50KB+) and complexity — TinyCongress's queries are simple enough that TanStack Query + a fetch wrapper provides the same functionality with less code and fewer abstractions to maintain
- Apollo's normalized cache is valuable for large apps with overlapping queries; TinyCongress doesn't have that pattern yet

### Build-time configuration only (`import.meta.env`)
- Simpler — no runtime injection, no `config.js`, no entrypoint script
- Rejected because it requires a separate Docker image per environment. The frontend image would need to be rebuilt for staging, production, and every preview deployment.

### Synchronous WASM loading (top-level await)
- Simpler component code — no provider, no loading state
- Rejected because top-level await blocks the entire module graph. A slow WASM load would delay rendering of components that don't need crypto. The provider pattern isolates the loading boundary.

### Single codegen pipeline (GraphQL only, REST types manual)
- Fewer build tools
- Rejected because manual REST types drift from the backend. `openapi-typescript` is low-overhead and guarantees type correctness for REST responses including error types.

### Server-side rendering (Next.js/Remix)
- Better initial load performance, SEO (if needed)
- Rejected for complexity — TinyCongress is a client-side application with WASM crypto that must run in the browser. SSR adds a Node.js server tier without clear benefit for an authenticated SPA.

## References
- [ADR-005: Mantine-First Styling](005-mantine-first-styling.md) — UI component library choice
- [ADR-006: WASM Crypto Sharing](006-wasm-crypto-sharing.md) — the shared tc-crypto crate loaded by CryptoProvider
- [ADR-012: Dual API Surface](012-dual-api-surface.md) — the two API specs consumed by codegen
- `web/src/api/graphqlClient.ts` — GraphQL fetch wrapper
- `web/src/api/generated/graphql.ts` — generated GraphQL types and Zod schemas
- `web/src/api/generated/rest.ts` — generated REST types
- `web/src/api/queries.ts` — queryOptions definitions
- `web/src/providers/QueryProvider.tsx` — TanStack Query configuration
- `web/src/providers/CryptoProvider.tsx` — WASM loading provider
- `web/src/config.ts` — runtime configuration
- `web/codegen.ts` — GraphQL codegen configuration

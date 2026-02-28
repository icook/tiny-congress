# GraphQL Codegen Workflow

## When to use
- After modifying GraphQL types in Rust (adding/changing queries, mutations, or types)
- After updating the GraphQL schema in any way
- Before committing changes that affect the API contract

## Prerequisites
- Rust toolchain installed
- Node.js and Yarn installed (`just setup` to verify)
- Backend compiles: `just build-backend`

## How it works

The codegen pipeline has two stages:

1. **Schema export** (Rust → GraphQL SDL)
   - `service/src/bin/export_schema.rs` introspects the async-graphql schema
   - Outputs to `web/schema.graphql`

2. **Type generation** (GraphQL SDL → TypeScript)
   - `web/codegen.ts` configures @graphql-codegen
   - Generates TypeScript types and Zod validation schemas
   - Outputs to `web/src/api/generated/graphql.ts`

## Steps

1. **Make your Rust changes** (add/modify types, queries, mutations)

2. **Run the full codegen**:
   ```bash
   just codegen
   ```

   This exports schemas from Rust and generates TypeScript types.

3. **Verify the generated types** in `web/src/api/generated/graphql.ts`

4. **Commit all generated files**:
   ```bash
   git add web/schema.graphql web/src/api/generated/
   git commit -m "chore: regenerate GraphQL types"
   ```

## Verification
- [ ] `just codegen` produces no additional changes (idempotent)
- [ ] `just lint-frontend` passes (generated code is formatted)
- [ ] `just lint-typecheck` passes
- [ ] CI `codegen-check` job will verify this automatically

## CI enforcement

The CI pipeline includes a `codegen-check` job that:
1. Runs `just codegen`
2. Fails if there are uncommitted changes to generated files

If CI fails with "GraphQL codegen is out of date", run `just codegen` locally and commit the changes.

## What gets generated

The `web/src/api/generated/graphql.ts` file contains:

- **TypeScript types** for all GraphQL types (Query, Mutation, input types, etc.)
- **Zod schemas** for runtime validation (e.g., `UserSchema`, `IssueSchema`)
- **Enum unions** as string literal types

Example usage in frontend code:
```typescript
import { User, UserSchema, Issue } from '@/api/generated/graphql';

// Type-safe API response handling
const user: User = response.data.currentUser;

// Runtime validation
const validatedUser = UserSchema.parse(apiResponse);
```

## Common failures

| Error | Cause | Fix |
|-------|-------|-----|
| "schema.graphql not found" | Schema not exported | Run `just codegen` (exports + generates) |
| Type mismatch in frontend | Stale generated types | Run `just codegen` |
| CI codegen-check fails | Forgot to commit generated files | Run `just codegen` and commit |
| Rust compile error in export_schema | Schema has issues | Fix Rust code, then re-export |

## Prohibited actions
- DO NOT manually edit `web/schema.graphql` or `web/src/api/generated/graphql.ts`
- DO NOT skip codegen when changing GraphQL types

## See also
- `web/codegen.ts` - codegen configuration
- `service/src/bin/export_schema.rs` - schema export binary
- [Adding a New GraphQL Endpoint](./new-graphql-endpoint.md)

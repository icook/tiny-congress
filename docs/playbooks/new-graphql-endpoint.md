# Adding a New GraphQL Endpoint

## When to use
- Adding new queries or mutations to the API
- Exposing new data to the frontend

## Prerequisites
- Backend compiles: `just build-backend`
- Understanding of existing resolver patterns in `service/src/`

## Steps

1. **Define the schema** in resolver module:
   ```rust
   // In the appropriate resolver file
   #[Object]
   impl QueryRoot {
       async fn my_new_query(&self, ctx: &Context<'_>) -> Result<MyType> {
           // implementation
       }
   }
   ```

2. **Add any new types** needed:
   ```rust
   #[derive(SimpleObject)]
   pub struct MyType {
       pub id: i32,
       pub name: String,
   }
   ```

3. **Write database query** if needed (sqlx):
   ```rust
   sqlx::query_as!(MyType, "SELECT id, name FROM my_table WHERE ...")
       .fetch_all(pool)
       .await?
   ```

4. **Regenerate sqlx cache** if queries changed:
   ```bash
   cd service
   cargo sqlx prepare
   ```

5. **Add tests** in `service/tests/`:
   ```rust
   use tc_test_macros::shared_runtime_test;
   use common::test_db::test_transaction;

   #[shared_runtime_test]
   async fn test_my_new_query() {
       let mut tx = test_transaction().await;
       // Setup and assertions using &mut *tx
   }
   ```
   Use `#[shared_runtime_test]` with `test_transaction()` for database tests.
   See [Backend Test Patterns](./backend-test-patterns.md) for detailed guidance.

6. **Regenerate TypeScript types**:
   ```bash
   just codegen
   ```
   This exports the schema and generates TypeScript types + Zod schemas.

7. **Update frontend** to use new endpoint (see `web/src/`)

## Verification
- [ ] `just test-backend` passes
- [ ] `just lint-backend` clean
- [ ] `just codegen` produces no additional changes
- [ ] Endpoint accessible via GraphQL playground
- [ ] Frontend integration works

## Common failures

| Error | Cause | Fix |
|-------|-------|-----|
| "field not found" | Type mismatch with DB | Check sqlx query column names |
| Resolver not visible | Not registered in schema | Add to QueryRoot/MutationRoot |
| N+1 queries | Missing dataloader | Use async-graphql dataloaders |

## Prohibited actions
- DO NOT delete or rename existing public endpoints without deprecation
- DO NOT expose internal IDs without authorization checks

## See also
- `service/src/` - existing resolvers
- [GraphQL Codegen Workflow](./graphql-codegen.md) - type generation process
- [Backend Test Patterns](./backend-test-patterns.md) - database and resolver testing
- [Test Writing Skill](../skills/test-writing.md) - LLM decision tree for test placement
- async-graphql docs for patterns

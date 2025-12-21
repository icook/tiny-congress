# Test Writing

Use this skill when writing tests for new code or bug fixes. This guide helps you choose the right test type and location.

## Decision Tree

```
What are you testing?
│
├─► Backend (Rust code in service/)
│   │
│   ├─► Database interaction?
│   │   │
│   │   ├─► Query logic, CRUD, business logic
│   │   │   → Use `test_transaction()` with `#[shared_runtime_test]`
│   │   │
│   │   └─► Migration, transaction isolation, DB features
│   │       → Use `isolated_db()` with `#[shared_runtime_test]`
│   │
│   ├─► GraphQL resolver?
│   │   → Test via schema.execute() or full HTTP test
│   │   → See backend-test-patterns.md#graphql-resolver-tests
│   │
│   ├─► Pure function, no DB?
│   │   → Use `#[test]` or `#[tokio::test]`
│   │
│   └─► Invariants that should hold for all inputs?
│       → Use proptest for property-based testing
│
└─► Frontend (TypeScript code in web/)
    │
    ├─► Component rendering/interaction?
    │   → Vitest test in same directory: `Component.test.tsx`
    │   → Use `@test-utils` for render/screen/userEvent
    │
    ├─► User flow across pages?
    │   → Playwright E2E in `web/tests/`
    │   → Use `./fixtures` for shared helpers
    │
    └─► Time-dependent behavior?
        → Use fake timers (vi.useFakeTimers or page.clock)
```

## Quick Reference

| Scenario | Test Type | Location | Macro/Tool |
|----------|-----------|----------|------------|
| DB query/CRUD | Integration | `service/tests/*_tests.rs` | `#[shared_runtime_test]` + `test_transaction()` |
| Migration testing | Isolated DB | `service/tests/*_tests.rs` | `#[shared_runtime_test]` + `isolated_db()` |
| GraphQL resolver | Integration | `service/tests/graphql_tests.rs` | `#[shared_runtime_test]` or `#[tokio::test]` |
| Pure Rust function | Unit | `service/tests/*_tests.rs` | `#[test]` |
| Async without DB | Unit | `service/tests/*_tests.rs` | `#[tokio::test]` |
| Property/invariant | Property | Same file, `mod proptests` | `proptest!` |
| React component | Unit | `web/src/**/*.test.tsx` | Vitest |
| User flow E2E | E2E | `web/tests/*.spec.ts` | Playwright |

## Backend: Which Database Pattern?

**95% of tests:** Use `test_transaction()`
- Fast (~1-5ms setup)
- Auto-rollback on drop
- Ideal for query logic, CRUD, business rules

**5% of tests:** Use `isolated_db()`
- Slower (~15-30ms setup)
- Full database isolation
- Use for: migrations, concurrent transactions, LISTEN/NOTIFY, advisory locks

**Never use `get_test_db()` for tests that write data** - changes persist and cause flaky tests.

## Frontend: Component vs E2E

**Component tests (Vitest):**
- Single component behavior
- Unit interactions (clicks, inputs)
- Fast, no browser needed
- Co-located with component

**E2E tests (Playwright):**
- Multi-page user journeys
- Integration with real API
- Visual verification
- In `web/tests/` directory

## Naming Conventions

```
# Backend
service/tests/
  {feature}_tests.rs    # e.g., voting_tests.rs, auth_tests.rs

# Frontend
web/src/components/
  MyComponent.tsx
  MyComponent.test.tsx  # Co-located

web/tests/
  user-signup.spec.ts   # E2E kebab-case
```

## See Also

- [Backend Test Patterns](../playbooks/backend-test-patterns.md) - Detailed Rust testing guide
- [Frontend Test Patterns](../playbooks/frontend-test-patterns.md) - Vitest and Playwright patterns
- [Test Data Factories](../playbooks/test-data-factories.md) - Creating backend test data

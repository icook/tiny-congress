# Codebase Sweep

Use this skill to systematically audit the codebase for quality issues, anti-patterns, and structural drift. A sweep is a research-only pass — it produces a prioritized findings report, not code changes.

## When to Use

- After a burst of feature work (multiple PRs landed)
- Before a milestone demo or release
- When onboarding to assess codebase health
- Periodically (monthly) as hygiene

## Sweep Categories

Run all categories in parallel using subagents. Each agent gets one category and returns a structured report.

### 1. Type Safety & Domain Modeling (backend)

Look for:
- `String` where a newtype exists (`Kid`, `Username`, `RootPubkey`, `BackupEnvelope`)
- Missing validation in newtype constructors (accepts any input without `TryFrom`)
- Lossy `as` casts that could silently truncate
- Primitive obsession: multiple `&str` or `Uuid` params representing distinct domain concepts (easy to swap)

Search: `service/src/`, `crates/`. Skip test files.

### 2. Error Handling (backend)

Look for:
- `anyhow` in service/repo/library code (should be `thiserror` enums)
- Swallowed errors: `.ok()`, `let _ =`, `.unwrap_or_default()` hiding failures that matter
- `unwrap()`/`expect()` in non-test code without justification
- `Internal(String)` variants that always carry the same generic message
- Structured errors erased to `anyhow::anyhow!("...")` (information loss)
- Modules mixing both `anyhow` and `thiserror`

Search: `service/src/`, `crates/`. Skip test files for unwrap checks.

### 3. Security & Trust Boundary

Look for:
- Non-constant-time comparison (`==`/`!=`) on secrets, tokens, HMAC tags, nonces
- Missing input validation at HTTP boundaries (length, format, range)
- Leaked internals in error responses (DB errors, stack traces in HTTP bodies)
- Trust boundary violations: server-side `SigningKey` generation, encryption, or private key handling outside `#[cfg(test)]`
- Missing rate limiting on auth endpoints
- TOCTOU races: check-then-act without transactions

Search: `service/src/`, `crates/tc-crypto/`.

### 4. SQL & Data Access (backend)

Look for:
- `Extension<PgPool>` in HTTP handlers (must use `Arc<dyn *Repo>` traits)
- N+1 queries: fetch list then loop-and-query
- Unchecked `.execute()` results on UPDATE/DELETE (missing `rows_affected()`)
- `format!` building SQL strings (must use parameterized queries)
- Multi-step operations without transaction boundaries
- Repo trait gaps forcing handlers to bypass abstraction
- Missing indexes on columns used in WHERE clauses

Search: `service/src/`, `service/migrations/`.

### 5. Dead Code & Speculative Design

Look for:
- `#[allow(dead_code)]` — is the code actually used?
- Commented-out code blocks (3+ lines)
- `TODO`/`FIXME`/`HACK` without ticket numbers
- Dispatch logic for nonexistent variants (enum with one variant but full match)
- Unused struct fields (deserialized but never read)
- Stub implementations that panic if called

Search: `service/src/`, `crates/`, `web/src/`.

### 6. Frontend Consistency

Look for:
- Mock drift: test mock shapes that don't match real API response types
- Tests bypassing `@test-utils` with manual `MantineProvider`/`QueryClientProvider`
- Duplicated API hooks across feature directories
- Query key misalignment: same endpoint, different cache keys
- Missing `isError` handling on `useQuery` results (only `isLoading`)
- Inline styles where Mantine props exist
- `as any` or untyped API responses in production code
- Cross-feature deep relative imports (`../../other-feature/internal`)

Search: `web/src/`.

### 7. Test Quality

Look for:
- Happy-path-only tests: `Result`-returning functions with no `Err` case coverage
- Assertion-free tests: code runs but nothing is asserted
- Missing boundary inputs: empty strings, zero, max length, unicode
- Flaky time-dependent tests: `Utc::now()` in assertions without tolerance
- Duplicate test coverage: multiple tests exercising the same code path

Search: `service/src/`, `service/tests/`, `crates/`, `web/src/`.

### 8. Config & Environment Variable Hygiene

Look for:
- Required config fields with no validation in `Config::validate()` (silent empty strings)
- `#[derive(Debug)]` on structs containing secrets (footgun if `{:?}` formatted)
- Helm values not wired to configmap/env vars (app expects a value, deployment doesn't set it)
- Duplicated constants across Rust defaults and Helm values (two sources of truth)
- Hardcoded ports/URLs in templates that should reference values
- `${VAR:-default}` in entrypoints where `${VAR:?}` is appropriate

Search: `service/src/config.rs`, `kube/`, `skaffold.yaml`, `dockerfiles/`.

### 9. API Contract & Schema Drift

Look for:
- Endpoints in code not in `web/openapi.json` (or vice versa)
- Response struct field names that don't match frontend TypeScript types (snake_case mismatch, renamed fields)
- Response wrapper mismatches: backend wraps in `{ items: [...] }`, frontend expects bare array
- Dead frontend API functions calling removed endpoints
- Dead backend endpoints with zero frontend callers

Search: `web/openapi.json`, `web/schema.graphql`, `service/src/*/http/`, `web/src/api/`.

### 10. Migration ↔ Code Sync

Look for:
- DB columns in migrations not represented in Rust `FromRow` structs
- Rust struct fields referencing columns that don't exist in any migration
- CHECK constraints in SQL not enforced in Rust validation (opaque DB errors vs clean 400s)
- NOT NULL columns with no Rust-side presence check
- Dead columns from removed features still in the schema
- Enum/status CHECK values that don't match Rust string literals

Search: `service/migrations/*.sql`, `service/src/*/repo/`, `crates/*/src/repo/`.

### 11. Middleware & Request Pipeline

Look for:
- Security headers missing on some routes (applied before merge instead of after)
- No `DefaultBodyLimit` on unauthenticated endpoints
- Unauthenticated mutation endpoints (missing `AuthenticatedDevice` extractor)
- Rate limiting gaps (endpoints that should be limited but aren't)
- Extension availability ordering issues

Search: `service/src/main.rs`, `service/src/http/`, `service/src/*/http/mod.rs`.

### 12. DRY Violations & Consolidation

Look for:
- Copy-pasted handler patterns (same extract→validate→call→map-error structure)
- Duplicated error mapping across handlers (same repo error → same HTTP response in 3+ places)
- Near-identical SQL queries differing by one clause
- Duplicated validation logic in both service and HTTP layers
- Frontend: duplicated query guard boilerplate, duplicated component logic

Focus on findings where consolidation removes 10+ lines. Estimate lines removable per finding.

Search: `service/src/`, `crates/`, `web/src/`.

### 13. Concurrency & Race Conditions

Look for:
- Shared mutable state: `Arc<Mutex<>>`, `Arc<RwLock<>>` — locks held across `.await`?
- Background worker idempotency: stuck `processing` rows, no requeue mechanism
- `std::thread::sleep` or synchronous file I/O in async code
- TOCTOU between concurrent handlers (double-vote, double-signup)
- Connection pool exhaustion under concurrent load

Search: `service/src/`, `crates/`.

### 14. Resilience & Graceful Degradation

Look for:
- Missing timeouts on `reqwest::Client` calls (external HTTP hangs indefinitely)
- No exponential backoff on worker retries
- Background workers not joined on shutdown (fire-and-forget `tokio::spawn`)
- No circuit breaker on DB pool exhaustion
- Frontend: no request timeout via `AbortController`, no global "API unreachable" indicator

Search: `service/src/main.rs`, `service/src/trust/worker.rs`, `web/src/api/fetchClient.ts`.

### 15. Logging Hygiene

Look for:
- Sensitive data in logs: PII, tokens, secrets, connection strings
- `#[derive(Debug)]` on config structs containing secrets
- No request correlation middleware (can't link log lines to a request)
- Freeform error messages vs structured key=value fields
- Missing startup context (host, port, db name not logged)

Search: `service/src/`, grep for `tracing::` calls.

### 16. Frontend State Management

Look for:
- Mutations that don't invalidate affected query keys
- Memory leaks: `setInterval`/`setTimeout` without cleanup, event listeners not removed
- React key props using array index instead of stable ID
- Stale closures in `useCallback`/`useMemo` with missing dependencies
- No global handler for 401 responses (expired auth mid-session)

Search: `web/src/`.

### 17. Performance Anti-patterns

Look for:
- Unnecessary `.clone()` on hot paths (handlers, service calls)
- Blocking in async: `std::thread::sleep`, `std::fs::*` without `spawn_blocking`
- N+1 at frontend: list components where each item triggers own `useQuery`
- Queries using `COUNT(*)` where `EXISTS` would suffice
- Missing pagination on list endpoints

Search: `service/src/`, `crates/`, `web/src/`.

### 18. Test-Production Parity

Look for:
- Mock implementations that skip validation the real code enforces
- Test config that disables features enabled in production (beyond rate limiting)
- Frontend mock response shapes that don't match real API responses
- Mock `Ok(())` defaults that mask error path regressions

Search: `service/src/`, `service/tests/`, `web/src/`.

## Agent Dispatch Template

For a full sweep, launch all 18 categories. For a targeted sweep, pick the categories most relevant to recent changes. Recommended groupings:

- **Post-feature-burst**: 1–7 (core quality)
- **Pre-release hardening**: 3, 8, 10, 11, 13, 14, 15 (security & resilience)
- **De-slop / consolidation**: 5, 6, 9, 12, 16, 17 (cleanup & consistency)

Launch sonnet subagents in parallel, one per category. Each agent prompt should include:

1. The category checklist above
2. "This is a research-only task — do NOT edit any files"
3. "Report findings with file:line references"
4. "Rate severity: critical / high / medium / low"
5. "Return a structured report with: Category, File:line, Description, Severity, Suggested fix"

## Output Format

Compile agent results into a consolidated report:

```markdown
## Sweep Report — YYYY-MM-DD

### Critical / High Findings
| # | Category | File:Line | Description | Severity |
|---|----------|-----------|-------------|----------|

### Medium Findings
| # | Category | File:Line | Description | Severity |
|---|----------|-----------|-------------|----------|

### Low Findings
(summarize in prose, don't table every item)

### Proposed Actions
- **Fix now**: Mechanical fixes, file as a single "sweep cleanup" PR
- **File tickets**: Non-mechanical issues needing design decisions
- **Track**: Known gaps acceptable at current stage (note why)
```

## After the Sweep

1. **Mechanical fixes** — dispatch parallel worktree agents for obvious fixes (test setup, dead code, missing validation)
2. **File tickets** — create GitHub issues for design-level findings, using labels from `docs/interfaces/ticket-management.md`
3. **Update guardrails** — if a pattern appears 3+ times, add to `just lint-patterns`, `AGENTS.md`, and `scripts/refine-guidance.md`
4. **Update this skill** — if you discover a new category worth checking, add it

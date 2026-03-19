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

## Agent Dispatch Template

Launch 7 sonnet subagents in parallel, one per category. Each agent prompt should include:

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

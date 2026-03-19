# Refinement Guidance

This document tells the refinement agent what to look for. It supplements
the linter and CI checks — focus on things machines can't catch mechanically.

## Project Standards

Read these before making changes:
- `docs/interfaces/rust-coding-standards.md` — Rust patterns, newtype usage, error handling
- `docs/interfaces/react-coding-standards.md` — React/TS patterns
- `docs/interfaces/error-handling.md` — Structured error handling
- `docs/interfaces/secure-defaults.md` — Security configuration policy

## Pattern Enforcement (priority: high)

- **Newtypes over primitives.** If a `String` has format rules (e.g., a key ID,
  a nonce, a backup code), it should be a newtype with validation in its
  constructor. The pattern is: `pub struct Foo(String)` with `TryFrom<&str>`.
- **One code path.** If two functions do the same thing with slight variations,
  consolidate. Two paths are a bug that hasn't diverged yet.
- **Error types.** Prefer `thiserror` enums over `anyhow` in library code.
  `anyhow` is fine in handlers/resolvers. Don't mix both in the same module.
- **Shared HTTP helpers.** Error responses MUST use `crate::http::{bad_request,
  not_found, unauthorized, internal_error}`. Never construct
  `(StatusCode, Json(ErrorResponse {...})).into_response()` inline or use
  `serde_json::json!({"error": ...})` for error bodies.

### Structural Consistency Checks

Before looking for LLM-judgment improvements, run these mechanical checks
against the focus area. If any match, fix them — they are always worth a PR.

**Anti-patterns to grep for:**

| Anti-pattern | How to detect | Fix |
|---|---|---|
| Inline error response | `Json(ErrorResponse {` outside `http/mod.rs` | Use `crate::http::{bad_request,not_found,internal_error}` |
| Inconsistent JSON errors | `serde_json::json!.*error` in handler files | Use `crate::http::ErrorResponse` |
| Duplicated StatusCode mapping | Same `StatusCode::X, Json(ErrorResponse` in 2+ files | Extract to shared error mapper or use helpers |

Run the equivalent grep commands against your focus path before engaging
LLM judgment. If you find matches, that IS the highest-value improvement.
Do NOT classify structural inconsistency as "cleanup" (low impact) — it
causes drift and makes every future handler copy the wrong pattern. Rate
it medium at minimum.

### When to propose a guardrail

If you find the same anti-pattern in 3+ files across different modules, that's
not a code problem — it's a missing guardrail. Use action `"ticket"` to propose:
1. A CLAUDE.md rule (what to do instead)
2. A grep-based lint for `just lint-patterns` (how to catch it mechanically)
3. An update to this guidance file (so future refine runs catch it)

The ticket body should include the proposed rule text, not just describe the problem.

## Security Hardening (priority: high)

- **Boundary validation.** Every public function that accepts external input
  must validate it. Check length, format, range. Reject, don't sanitize.
- **Fail closed.** If a check is ambiguous, reject. A crash from a violated
  assumption is better than silent acceptance.
- **No string comparison for secrets.** Use constant-time comparison for
  anything secret-adjacent (tokens, hashes, nonces).

## Test Coverage (priority: medium)

- **Error paths.** If a function returns `Result`, test the `Err` case.
- **Boundary inputs.** Test empty strings, zero values, maximum lengths.
- **Follow existing patterns.** Backend tests use `#[shared_runtime_test]`
  with `test_transaction()` or `isolated_db()`. Frontend tests use Vitest.
  See `docs/skills/test-writing.md` for the decision tree.

## Code Cleanup (priority: low)

- **Dead code.** Remove unused imports, functions, struct fields. Don't leave
  `#[allow(dead_code)]` — either use it or delete it.
- **Simplification.** Replace complex boolean expressions with named variables
  or early returns. Replace nested `match` with `if let` when there's one arm.
- **TODOs.** If a TODO is now actionable (the code it references exists), fix
  it. If it's aspirational, leave it alone.

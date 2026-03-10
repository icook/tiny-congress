# PR Checklist

Use as author before opening a PR, and as reviewer when reviewing.

## Shared Checks (Author & Reviewer)

### Code Quality
- [ ] Changes match the issue/ticket requirements
- [ ] No unrelated changes bundled
- [ ] No debug code (console.log, dbg!, println!)
- [ ] No commented-out code without explanation

### Testing
- [ ] Tests added for new functionality
- [ ] Existing tests updated if behavior changed
- [ ] CI passes

### Security
- [ ] No secrets or credentials in code
- [ ] No hardcoded API keys or tokens
- [ ] Input validation for user-provided data
- [ ] SQL queries use parameterized statements
- [ ] No SQL injection or XSS vectors

### Database (if applicable)
- [ ] Migration tested locally
- [ ] Rollback documented
- [ ] `.sqlx/` regenerated: `cargo sqlx prepare`
- [ ] No breaking schema changes without expand-contract

### Dependencies (if applicable)
- [ ] Lockfile committed (`Cargo.lock` or `yarn.lock`)
- [ ] Security audit: `cargo audit` / `yarn audit`
- [ ] License compatible with project

### Agent-Specific (if agent-generated)
- [ ] Compliance block in PR description
- [ ] CLAUDE.md listed in docs_read
- [ ] files_modified matches actual diff
- [ ] All prohibited actions reviewed

## Author Only

Complete before opening or marking PR ready for review.

- [ ] No TODO comments without linked issue
- [ ] Rust code follows [rust-coding-standards.md](../interfaces/rust-coding-standards.md)
- [ ] Manual testing completed for UI changes
- [ ] All linting passes (includes typecheck): `just lint`
- [ ] Code comments for non-obvious logic
- [ ] README updated if setup changed
- [ ] Playbook added/updated if new workflow introduced
- [ ] ADR created if architectural decision made
- [ ] Branch name follows convention (see [branch-naming-conventions.md](../interfaces/branch-naming-conventions.md))
- [ ] Commits are focused and atomic
- [ ] Commit messages follow convention (imperative, concise)
- [ ] Branch rebased on latest master
- [ ] No merge commits in feature branch
- [ ] PR description explains why, not just what
- [ ] Issue linked in PR description
- [ ] `just test-ci` passes locally (full CI suite)

## Reviewer Only

### Cross-reference with CLAUDE.md
- [ ] Files modified are in allowed directories
- [ ] No new tables without approval
- [ ] No skaffold.yaml changes without skill verification

### Common agent mistakes
- [ ] No invented file paths
- [ ] No hallucinated APIs or functions
- [ ] No over-engineering beyond request
- [ ] No security vulnerabilities introduced

### By change type

**API changes:**
- [ ] No breaking changes to existing endpoints
- [ ] New endpoints have tests
- [ ] Error handling consistent

**Frontend changes:**
- [ ] Components have tests
- [ ] No hardcoded strings (use i18n if applicable)
- [ ] Responsive design considered
- [ ] Accessibility basics (labels, aria, keyboard nav)

**Infrastructure changes:**
- [ ] testing-local-dev skill was run
- [ ] Changes tested with `skaffold verify -p ci`
- [ ] No secrets exposed

## Red Flags (request changes)

- [ ] Secrets or credentials in code
- [ ] `--no-verify` or `-f` flags in commits
- [ ] Direct commits to master
- [ ] Missing tests for complex logic
- [ ] Unhandled error cases

## Approval Criteria

**Approve if:** All shared checks pass, no red flags, agent compliance verified (for agent PRs).

**Request changes if:** Any red flag present, compliance block missing/invalid (agent PRs), tests missing for new functionality.

## See Also
- CLAUDE.md - Prohibited Actions

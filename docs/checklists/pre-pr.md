# Pre-PR Checklist

Complete before opening or marking PR ready for review.

## Code quality

- [ ] Changes match the issue/ticket requirements
- [ ] No unrelated changes included
- [ ] No debug code (console.log, dbg!, println!)
- [ ] No commented-out code without explanation
- [ ] No TODO comments without linked issue
- [ ] Rust code follows [rust-coding-standards.md](../interfaces/rust-coding-standards.md)

## Testing

- [ ] Tests added for new functionality
- [ ] Tests updated for changed behavior
- [ ] All tests pass locally: `just test`
- [ ] Manual testing completed for UI changes

## Linting

- [ ] All linting passes (includes typecheck): `just lint`

## Documentation

- [ ] Code comments for non-obvious logic
- [ ] README updated if setup changed
- [ ] Playbook added/updated if new workflow introduced
- [ ] ADR created if architectural decision made

## Security

- [ ] No secrets or credentials in code
- [ ] No hardcoded API keys or tokens
- [ ] Input validation for user-provided data
- [ ] SQL queries use parameterized statements

## Database (if applicable)

- [ ] Migration tested locally
- [ ] Rollback documented
- [ ] `.sqlx/` regenerated: `cargo sqlx prepare`
- [ ] No breaking schema changes without expand-contract

## Dependencies (if applicable)

- [ ] Lockfile committed (`Cargo.lock` or `yarn.lock`)
- [ ] Security audit: `cargo audit` / `yarn audit`
- [ ] License compatible with project

## Git hygiene

- [ ] Branch name follows convention (see `../interfaces/branch-naming-conventions.md`)
- [ ] Commits are focused and atomic
- [ ] Commit messages follow convention (imperative, concise)
- [ ] Branch rebased on latest master
- [ ] No merge commits in feature branch

## Agent-specific (if agent-generated)

- [ ] Compliance block added to PR description
- [ ] AGENTS.md listed in docs_read
- [ ] files_modified matches actual diff
- [ ] All prohibited actions reviewed

## Final steps

- [ ] `just test-ci` passes locally (full CI suite)
- [ ] PR description explains why, not just what
- [ ] Issue linked in PR description
- [ ] Appropriate reviewers assigned

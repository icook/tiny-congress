# PR Review Checklist

## When to use
- Reviewing PRs (human or agent-generated)
- Self-review before marking PR ready

## Quick checks (all PRs)

### Compliance
- [ ] PR links to issue (if applicable)
- [ ] Agent PRs have compliance block (see AGENTS.md)
- [ ] No prohibited actions violated

### Code quality
- [ ] Changes match PR description
- [ ] No unrelated changes bundled
- [ ] No debug code left in (console.log, dbg!, etc.)
- [ ] No commented-out code without explanation

### Testing
- [ ] Tests added for new functionality
- [ ] Existing tests updated if behavior changed
- [ ] CI passes

## Agent-specific checks

### Verify compliance block
```yaml
agent_compliance:
  docs_read:
    - AGENTS.md           # Required
  constraints_followed: true
  files_modified: [...]   # Must match actual diff
  deviations:
    - none                # Or valid explanation
```

### Cross-reference with AGENTS.md
- [ ] Files modified are in allowed directories
- [ ] No new tables without approval
- [ ] No skaffold.yaml changes without skill verification
- [ ] Dependencies have lockfile updates

### Common agent mistakes
- [ ] No invented file paths
- [ ] No hallucinated APIs or functions
- [ ] No over-engineering beyond request
- [ ] No security vulnerabilities introduced

## By change type

### Database changes
- [ ] Migration is reversible or has rollback documented
- [ ] No breaking schema changes without expand-contract
- [ ] sqlx prepare regenerated

### API changes
- [ ] No breaking changes to existing endpoints
- [ ] New endpoints have tests
- [ ] Error handling consistent

### Frontend changes
- [ ] Components have tests
- [ ] No hardcoded strings (use i18n if applicable)
- [ ] Responsive design considered
- [ ] Accessibility basics (labels, aria, keyboard nav)

### Infrastructure changes
- [ ] testing-local-dev skill was run
- [ ] Changes tested with `skaffold verify -p ci`
- [ ] No secrets exposed

## Red flags (request changes)

- [ ] Secrets or credentials in code
- [ ] `--no-verify` or `-f` flags in commits
- [ ] Direct commits to master
- [ ] Missing tests for complex logic
- [ ] Unhandled error cases
- [ ] SQL injection or XSS vectors

## Approval criteria

**Approve if:**
- All quick checks pass
- Agent compliance verified (for agent PRs)
- No red flags

**Request changes if:**
- Any red flag present
- Compliance block missing/invalid (agent PRs)
- Tests missing for new functionality

## See also
- AGENTS.md § Prohibited Actions
- AGENTS.md § Agent Acknowledgement Contract
- `.github/scripts/check-agent-compliance.sh`

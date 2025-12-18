# Branch Naming Conventions

## Overview

Branch names serve as documentation and communication tools. They should be:
- **Descriptive** - Clearly indicate the purpose of the work
- **Scannable** - Easy to identify in lists and logs
- **Consistent** - Follow predictable patterns
- **Trackable** - Link to issues when applicable

## Standard Format

```
<type>/<issue-number>-<description>
```

### Components

| Component | Required | Rules | Example |
|-----------|----------|-------|---------|
| `type` | Yes | Lowercase, from approved list | `feature`, `fix`, `refactor` |
| `issue-number` | When exists | GitHub issue number only | `123`, `4567` |
| `description` | Yes | kebab-case, 2-5 words | `add-voting-ui`, `fix-auth-loop` |

## Branch Types

### feature/
New functionality, enhancements, or additions to the codebase.

**Examples:**
```
feature/123-add-vote-counting
feature/456-member-dashboard
feature/789-export-reports
```

**Use when:**
- Adding new user-facing features
- Implementing new API endpoints
- Creating new components or modules
- Adding new integrations

### fix/
Bug fixes, error corrections, or issue resolutions.

**Examples:**
```
fix/234-login-redirect
fix/567-null-pointer-votes
fix/890-memory-leak
```

**Use when:**
- Fixing reported bugs
- Correcting unintended behavior
- Resolving errors or exceptions
- Patching security issues

### refactor/
Code improvements without changing behavior.

**Examples:**
```
refactor/111-extract-vote-logic
refactor/222-simplify-auth
refactor/333-remove-deprecated-api
```

**Use when:**
- Restructuring code organization
- Improving code quality or readability
- Removing dead code
- Renaming for clarity

### docs/
Documentation-only changes.

**Examples:**
```
docs/444-api-examples
docs/555-deployment-guide
docs/666-update-readme
```

**Use when:**
- Adding or updating documentation
- Fixing typos in docs
- Adding code examples
- Creating ADRs or playbooks

### test/
Test additions or improvements without production code changes.

**Examples:**
```
test/777-vote-integration
test/888-improve-coverage
test/999-flaky-auth-spec
```

**Use when:**
- Adding missing test coverage
- Fixing flaky tests
- Improving test infrastructure
- Adding E2E scenarios

### ci/
CI/CD pipeline, build, or tooling changes.

**Examples:**
```
ci/101-docker-cache
ci/202-add-playwright
ci/303-parallel-tests
```

**Use when:**
- Modifying GitHub Actions workflows
- Updating Skaffold configuration
- Changing Docker builds
- Adding linters or formatters

### chore/
Maintenance tasks, dependency updates, or administrative work.

**Examples:**
```
chore/112-update-deps
chore/223-bump-node-22
chore/334-cleanup-logs
```

**Use when:**
- Updating dependencies
- Bumping versions
- Cleaning up logs or comments
- Repository maintenance

### perf/
Performance optimizations and improvements.

**Examples:**
```
perf/445-optimize-queries
perf/556-reduce-bundle
perf/667-cache-votes
```

**Use when:**
- Optimizing database queries
- Reducing bundle size
- Improving runtime performance
- Adding caching layers

### security/
Security improvements, vulnerability fixes, or security-related updates.

**Examples:**
```
security/778-update-jwt
security/889-sanitize-inputs
security/990-audit-deps
```

**Use when:**
- Addressing security vulnerabilities
- Implementing security features
- Updating security-related dependencies
- Adding authentication/authorization

## Branch Naming Rules

### DO

- ✅ Keep descriptions concise (2-5 words)
- ✅ Use kebab-case for multi-word descriptions
- ✅ Include issue number when one exists
- ✅ Start with the most relevant type
- ✅ Use imperative verbs (`add`, `fix`, `update`)
- ✅ Be specific enough to identify the work

### DON'T

- ❌ Use underscores or camelCase: `feature/add_voting`, `feature/addVoting`
- ❌ Include your name: `feature/john-voting`
- ❌ Use generic descriptions: `feature/updates`, `fix/bugs`
- ❌ Include dates: `feature/2024-voting`
- ❌ Use special characters: `feature/voting!`, `fix/auth@issue`
- ❌ Make overly long names: `feature/add-comprehensive-voting-system-with-analytics`

## Special Cases

### No Issue Number

When working on tasks without a tracked issue, omit the issue number:

```
feature/improve-error-messages
fix/typo-in-readme
refactor/consolidate-utils
```

### Multiple Issues

If work spans multiple issues, use the primary issue number:

```
feature/123-user-dashboard
```

Reference other issues in commits or PR description.

### Hotfixes

For urgent production fixes, use `fix/` with descriptive context:

```
fix/critical-auth-bypass
fix/prod-database-timeout
```

### Experiments or Spikes

Use a descriptive approach with `spike/` or `experiment/`:

```
spike/graphql-vs-rest
experiment/new-ui-framework
```

### Release Branches

For release management (if needed):

```
release/v1.2.0
release/2024-q1
```

## Working with Branches

### Creating a Branch

Always start from updated `master`:

```bash
git checkout master
git pull --rebase
git checkout -b feature/123-add-voting
```

### Pushing a Branch

Always specify remote and branch explicitly (see ADR-004):

```bash
git push origin feature/123-add-voting
```

Never use bare `git push` or implicit branch references.

### Deleting After Merge

Clean up merged branches:

```bash
git branch -d feature/123-add-voting
git push origin --delete feature/123-add-voting
```

## Examples by Scenario

### Adding a New Feature

```
feature/234-member-search
feature/345-export-csv
feature/456-dark-mode
```

### Fixing a Bug

```
fix/567-login-timeout
fix/678-vote-count-wrong
fix/789-broken-styles
```

### Updating Documentation

```
docs/890-api-guide
docs/901-contributing
docs/012-adr-caching
```

### Improving Performance

```
perf/123-lazy-load-images
perf/234-optimize-queries
perf/345-reduce-rerender
```

### Refactoring Code

```
refactor/456-split-components
refactor/567-extract-hooks
refactor/678-rename-variables
```

## Integration with Issues

When creating a branch from an issue:

1. Note the issue number (e.g., #123)
2. Identify the work type (feature, fix, etc.)
3. Extract key terms from the issue title
4. Construct branch: `type/123-key-terms`

**Issue:** #234 "Add ability to search members by district"
**Branch:** `feature/234-search-by-district`

**Issue:** #345 "Login page redirects to wrong URL"
**Branch:** `fix/345-login-redirect`

## Validation Checklist

Before pushing a branch, verify:

- [ ] Type prefix is from the approved list
- [ ] Issue number is included (if applicable)
- [ ] Description uses kebab-case
- [ ] Description is 2-5 words
- [ ] No special characters except hyphens
- [ ] Clearly describes the work
- [ ] Follows `type/number-description` format

## Related Documentation

- `CLAUDE.md` - Repository guidelines and workflows
- `pr-naming-conventions.md` - PR titles and commit messages
- `naming-conventions.md` - General naming conventions
- `../decisions/004-explicit-git-push-branches.md` - Explicit push requirements
- `../playbooks/` - Development workflows

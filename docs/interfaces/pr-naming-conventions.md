# PR and Commit Naming Conventions

## Overview

PR titles and commit messages follow [Conventional Commits](https://www.conventionalcommits.org/) format. This enables clean git logs, automated changelog generation, and consistency across the codebase.

## Format

```
<type>(optional-scope): <imperative description> (#issue)
```

### Components

| Component | Required | Rules | Example |
|-----------|----------|-------|---------|
| `type` | Yes | Lowercase, from approved list | `feat`, `fix`, `refactor` |
| `scope` | No | Lowercase, identifies area | `auth`, `voting`, `docker` |
| `description` | Yes | Imperative mood, lowercase start | `add vote counting` |
| `issue` | When exists | GitHub issue number | `(#123)` |

## Types

| Type | Purpose | Branch Equivalent |
|------|---------|-------------------|
| `feat` | New functionality | `feature/` |
| `fix` | Bug fixes | `fix/` |
| `refactor` | Code restructuring (no behavior change) | `refactor/` |
| `docs` | Documentation only | `docs/` |
| `test` | Test additions or fixes | `test/` |
| `ci` | CI/CD and build changes | `ci/` |
| `chore` | Maintenance, dependencies | `chore/` |
| `perf` | Performance improvements | `perf/` |
| `security` | Security fixes or improvements | `security/` |

## Examples

### With Scope

```
feat(voting): add ranked choice support (#234)
fix(auth): resolve login redirect loop (#456)
refactor(api): extract response helpers (#217)
perf(queries): optimize member search (#789)
```

### Without Scope

```
docs: update API examples (#444)
chore: bump dependencies (#112)
test: improve coverage for vote module (#888)
ci: add parallel test execution (#303)
```

### Breaking Changes

Add `!` after type/scope for breaking changes:

```
feat(api)!: change vote endpoint response format (#567)
refactor!: rename VoteResult to BallotResult (#890)
```

## PR Title to Commit Message

GitHub populates squash commit messages from PR titles. Keep PR titles in this format so merged commits maintain a clean, consistent log.

**PR Title:** `feat(voting): add ranked choice support (#234)`
**Squash Commit:** Same as above, automatically

## Rules

### DO

- Use imperative mood: "add feature" not "added feature" or "adds feature"
- Keep descriptions concise (50 chars or less ideal)
- Include scope when it clarifies the change area
- Reference the issue number when one exists
- Start description with lowercase

### DON'T

- Use past tense: `feat: added voting`
- Use vague descriptions: `fix: bug fixes`
- Include redundant words: `feat: add new feature for voting`
- End with punctuation: `fix: resolve login issue.`
- Use scope for obvious context: `docs(documentation): update readme`

## Validation

Before submitting a PR, verify the title:

- [ ] Starts with valid type
- [ ] Scope (if used) accurately identifies the area
- [ ] Description uses imperative mood
- [ ] Description is concise and specific
- [ ] Issue number included (if applicable)
- [ ] No trailing punctuation

## Related Documentation

- `branch-naming-conventions.md` - Branch naming (types align)
- `naming-conventions.md` - General naming patterns
- `../../.github/pull_request_template.md` - PR body template

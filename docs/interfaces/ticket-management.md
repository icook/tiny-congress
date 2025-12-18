# Ticket Management & Labeling

## Overview

GitHub Issues are the source of truth for tracking work. This document defines how to create, label, triage, and manage tickets for consistency and discoverability.

**Goals:**
- Enable quick filtering and prioritization
- Provide clear context for contributors
- Support automation and reporting
- Integrate with branch naming and PR workflows

## Label Taxonomy

Labels are organized into **namespaced categories**. Every issue should have at least one label from `type/` and ideally one from `priority/`.

### Type Labels (Required)

What kind of work is this?

| Label | Color | Description | When to Use |
|-------|-------|-------------|-------------|
| `type/bug` | `#d73a4a` | Something isn't working | Broken functionality, errors, regressions |
| `type/feature` | `#a2eeef` | New functionality | New capabilities, user-facing features |
| `type/enhancement` | `#84b6eb` | Improvement to existing feature | UX improvements, performance, refinements |
| `type/docs` | `#0075ca` | Documentation only | Playbooks, ADRs, README updates |
| `type/refactor` | `#d4c5f9` | Code improvement, no behavior change | Technical debt, code organization |
| `type/test` | `#bfd4f2` | Test additions or fixes | Coverage, flaky tests, test infrastructure |
| `type/ci` | `#f9d0c4` | CI/CD pipeline changes | GitHub Actions, Skaffold, Docker builds |
| `type/chore` | `#c5def5` | Maintenance tasks | Dependency updates, cleanup |
| `type/security` | `#ee0701` | Security-related work | Vulnerabilities, auth, input validation |

### Priority Labels (Recommended)

How urgent is this?

| Label | Color | Description | SLA Guidance |
|-------|-------|-------------|--------------|
| `priority/critical` | `#b60205` | Production down, security breach | Address immediately |
| `priority/high` | `#d93f0b` | Significant impact, blocks work | Address this sprint |
| `priority/medium` | `#fbca04` | Important but not blocking | Address within 2-3 sprints |
| `priority/low` | `#0e8a16` | Nice to have, minor impact | Backlog, address opportunistically |

### Area Labels (Scope)

What part of the system does this affect?

| Label | Color | Description |
|-------|-------|-------------|
| `area/backend` | `#5319e7` | Rust API, GraphQL, database |
| `area/frontend` | `#1d76db` | React, UI components, styling |
| `area/infra` | `#006b75` | Kubernetes, Docker, deployment |
| `area/dx` | `#c7def8` | Developer experience, tooling |

### Effort Labels (Optional)

How much work is this?

| Label | Color | Description |
|-------|-------|-------------|
| `effort/small` | `#c2e0c6` | < 2 hours |
| `effort/medium` | `#fef2c0` | 2-8 hours |
| `effort/large` | `#f9d0c4` | 1-3 days |
| `effort/epic` | `#d4c5f9` | Multiple days, should be split |

### Special Labels

| Label | Color | Description |
|-------|-------|-------------|
| `good first issue` | `#7057ff` | Suitable for new contributors |
| `help wanted` | `#008672` | Actively seeking contributors |
| `wontfix` | `#ffffff` | Closed without fixing |
| `duplicate` | `#cfd3d7` | Duplicate of another issue |
| `needs-info` | `#d876e3` | Waiting for reporter clarification |
| `breaking` | `#b60205` | Introduces breaking changes |

## Issue Templates

### Bug Report

```markdown
## Bug Description
<!-- Clear description of the bug -->

## Steps to Reproduce
1.
2.
3.

## Expected Behavior
<!-- What should happen -->

## Actual Behavior
<!-- What actually happens -->

## Environment
- Browser/OS:
- Commit SHA:
- Relevant logs:

## Screenshots
<!-- If applicable -->
```

### Feature Request

```markdown
## Problem Statement
<!-- What problem does this solve? Who has this problem? -->

## Proposed Solution
<!-- How should this work? -->

## Alternatives Considered
<!-- What other approaches were considered? -->

## Additional Context
<!-- Mockups, examples, related issues -->
```

### Technical Task

```markdown
## Context
<!-- Why is this work needed? -->

## Proposed Changes
<!-- What will be changed? -->

## Acceptance Criteria
- [ ]
- [ ]
- [ ]

## Technical Notes
<!-- Implementation considerations, risks, dependencies -->
```

## Writing Good Tickets

### Title Format

```
[Component] Concise description in imperative mood
```

**Examples:**
- `[API] Add rate limiting to GraphQL endpoint`
- `[UI] Fix vote button alignment on mobile`
- `[Infra] Enable horizontal pod autoscaling`

**Anti-patterns:**
- ❌ `Bug in voting` (too vague)
- ❌ `The login page is broken and needs to be fixed ASAP!!!` (not actionable)
- ❌ `TCK-123: Update the thing` (don't duplicate issue numbers)

### Description Requirements

Every issue should include:

1. **Context** - Why does this matter? What's the background?
2. **Details** - Specific requirements, steps to reproduce, or acceptance criteria
3. **Scope** - What's in/out of scope for this ticket?

**Bug tickets** must include:
- Steps to reproduce
- Expected vs actual behavior
- Environment details (browser, OS, commit SHA)

**Feature tickets** must include:
- Problem statement (user need)
- Proposed solution
- Acceptance criteria

### Acceptance Criteria

Use checkboxes for testable criteria:

```markdown
## Acceptance Criteria
- [ ] API returns 429 status when rate limit exceeded
- [ ] Rate limit is configurable via environment variable
- [ ] Rate limiting is documented in API contracts
- [ ] Unit tests cover rate limit logic
```

## Triage Process

### Weekly Triage

1. Review newly created issues or issues missing required labels.
2. For each issue:
   - Add `type/` label
   - Add `area/` label
   - Add `priority/` label if determinable
   - Add `effort/` label if estimable
   - Request clarification with `needs-info` if unclear
   - Close with `duplicate` or `wontfix` if applicable
3. Assign or queue the issue for follow-up if it is ready for work.

### Triage Checklist

- [ ] Issue has clear title
- [ ] Issue has sufficient detail to act on
- [ ] `type/` label applied
- [ ] `area/` label applied
- [ ] `priority/` assessed (or marked for backlog review)
- [ ] Not a duplicate of existing issue
- [ ] If bug: reproduction steps present
- [ ] If feature: acceptance criteria defined

## Linking Issues to Work

### Branch Naming

Branch names reference issue numbers per `branch-naming-conventions.md`:

```
feature/123-add-rate-limiting
fix/456-mobile-alignment
```

### Commit References

Reference issues in commits:

```
Add rate limiting to GraphQL endpoint

Implements configurable rate limiting with Redis backend.
Defaults to 100 requests per minute per IP.

Refs #123
```

### PR Linking

Use closing keywords in PR descriptions:

```markdown
## Summary
Adds rate limiting to prevent API abuse.

## Closes
Closes #123
```

## Automation Opportunities

### Stale Issue Management

Issues without activity for 30 days should be:
1. Commented with a warning
2. Closed after 14 more days without response

### Auto-labeling

Consider GitHub Actions to:
- Add `area/backend` for changes in `service/`
- Add `area/frontend` for changes in `web/`

### Issue-PR Linking

When PR references an issue:
- When PR merges, automatically close issue

## Label Setup Script

Run this to create all labels:

```bash
#!/bin/bash
# Create label taxonomy

# Type labels
gh label create "type/bug" --color "d73a4a" --description "Something isn't working" --force
gh label create "type/feature" --color "a2eeef" --description "New functionality" --force
gh label create "type/enhancement" --color "84b6eb" --description "Improvement to existing feature" --force
gh label create "type/docs" --color "0075ca" --description "Documentation only" --force
gh label create "type/refactor" --color "d4c5f9" --description "Code improvement, no behavior change" --force
gh label create "type/test" --color "bfd4f2" --description "Test additions or fixes" --force
gh label create "type/ci" --color "f9d0c4" --description "CI/CD pipeline changes" --force
gh label create "type/chore" --color "c5def5" --description "Maintenance tasks" --force
gh label create "type/security" --color "ee0701" --description "Security-related work" --force

# Priority labels
gh label create "priority/critical" --color "b60205" --description "Production down, security breach" --force
gh label create "priority/high" --color "d93f0b" --description "Significant impact, blocks work" --force
gh label create "priority/medium" --color "fbca04" --description "Important but not blocking" --force
gh label create "priority/low" --color "0e8a16" --description "Nice to have, minor impact" --force

# Area labels
gh label create "area/backend" --color "5319e7" --description "Rust API, GraphQL, database" --force
gh label create "area/frontend" --color "1d76db" --description "React, UI components, styling" --force
gh label create "area/infra" --color "006b75" --description "Kubernetes, Docker, deployment" --force
gh label create "area/dx" --color "c7def8" --description "Developer experience, tooling" --force

# Effort labels
gh label create "effort/small" --color "c2e0c6" --description "< 2 hours" --force
gh label create "effort/medium" --color "fef2c0" --description "2-8 hours" --force
gh label create "effort/large" --color "f9d0c4" --description "1-3 days" --force
gh label create "effort/epic" --color "d4c5f9" --description "Multiple days, should be split" --force

# Special labels
gh label create "breaking" --color "b60205" --description "Introduces breaking changes" --force
gh label create "needs-info" --color "d876e3" --description "Waiting for reporter clarification" --force

echo "Labels created successfully"
```

## Quick Reference

### Minimum Viable Issue

```markdown
**Title:** [Area] Clear imperative description

**Labels:** type/*, priority/* (if known)

**Body:**
## Context
One paragraph explaining why this matters.

## Details
Specific requirements or reproduction steps.

## Acceptance Criteria
- [ ] Testable outcome 1
- [ ] Testable outcome 2
```

### Label Combinations by Scenario

| Scenario | Labels |
|----------|--------|
| Production bug | `type/bug` `priority/critical` `area/*` |
| New feature | `type/feature` `priority/*` `area/*` `effort/*` |
| Docs update | `type/docs` `priority/low` |
| Refactoring | `type/refactor` `area/*` `effort/*` |
| Security fix | `type/security` `priority/high` `area/*` |
| Flaky test | `type/test` `area/*` |
| CI improvement | `type/ci` `area/infra` |
| Dependency update | `type/chore` `area/*` |
| Good starter task | `type/*` `effort/small` `good first issue` |

## Related Documentation

- `branch-naming-conventions.md` - Branch names reference issue numbers
- `../checklists/pre-pr.md` - PR checklist references linked issues
- `../playbooks/pr-review-checklist.md` - Review process
- `../../.github/pull_request_template.md` - PR template with issue linking

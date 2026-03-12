# Feature Planning Documents

Planning documents for large feature branches or major subsystems.

## Purpose

- Feature specifications that evolve during implementation
- Design briefs, spike plans, and gap analyses
- Design decisions being refined before graduation to permanent docs

## Why Here Instead of GitHub Issues?

- Changes tracked in git history
- Evolves with the code in the same branch
- No context-switching to update external systems
- Easy to review spec changes alongside code changes

## Lifecycle

1. **Created during planning** — design briefs, spike briefs, gap analyses
2. **Committed to feature branch as first commit** — preserves context across sessions
3. **Evolves during implementation** — updated as decisions are made
4. **Graduated before merge** — durable knowledge moves to permanent docs:
   - Architectural decisions → `docs/decisions/NNN-*.md` (ADR)
   - Domain model changes → `docs/domain-model.md`
   - API contracts / interfaces → `docs/interfaces/*.md`
   - Operational procedures → `docs/playbooks/*.md`
   - UX patterns → `docs/style/*.md`
5. **Stripped on merge to master** — CI enforces this (`check-no-plans` job)

Use `docs/skills/plan-graduation.md` for the full graduation process.

## Structure

```
.plan/
├── README.md                    # This file (committed to master)
├── {date}-{feature}-brief.md    # Design brief for brainstorming
├── {date}-{feature}-spike.md    # Spike validation plan
└── {date}-{feature}-gap.md      # Gap analysis
```

See `AGENTS.md` for full specification.

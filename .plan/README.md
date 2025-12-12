# Feature Planning Documents

Planning documents for large feature branches or major subsystems.

## Purpose

- Feature specifications that evolve during implementation
- Ticket definitions and status tracking
- Design decisions being refined

## Why Here Instead of GitHub Issues?

- Changes tracked in git history
- Evolves with the code in the same branch
- No context-switching to update external systems
- Easy to review spec changes alongside code changes

## Lifecycle

- **Committed to feature branch** during development
- **Removed when feature merges to master**
- Promote persistent content to `docs/` before merge

## Structure

```
.plan/
├── spec.md           # Main feature specification
├── tickets.md        # Ticket breakdown and status
└── {subsystem}/      # Subsystem-specific planning if needed
```

See `AGENTS.md` for full specification.

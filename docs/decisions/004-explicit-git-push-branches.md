# ADR-004: Require explicit branch names on git push

## Status
Accepted

## Context
An agent ran `git push --force-with-lease` without specifying a branch, which force-pushed multiple branches including `master` due to push.default configuration. This required manual recovery of multiple refs.

Git's push.default setting and refspecs can cause unexpected behavior:
- `push.default=matching` pushes all matching branches
- `push.default=current` pushes current branch but to potentially wrong remote ref
- Force push without explicit ref can affect multiple branches

## Decision
All git push commands MUST specify the remote and branch explicitly:

```bash
# Correct
git push origin feature/my-branch
git push --force-with-lease origin feature/my-branch

# Prohibited
git push
git push --force-with-lease
git push origin HEAD
```

This is enforced as a hard constraint in AGENTS.md.

## Consequences

### Positive
- Eliminates accidental multi-branch pushes
- Makes push intent explicit and auditable
- Prevents force-push to wrong branch

### Negative
- More verbose commands
- Agents must track current branch name

### Neutral
- No change for well-behaved push workflows
- Human developers can still use shortcuts (at their own risk)

## Alternatives considered

### Configure push.default=nothing globally
- Requires git config on every machine/container
- Rejected: Can't enforce in all agent environments

### Git hooks to block bare push
- Pre-push hook could validate
- Rejected: Hooks can be bypassed, agents may not have hooks installed

### Branch protection only
- GitHub branch protection prevents force-push to master
- Rejected: Doesn't prevent pushing to wrong feature branches, protection was bypassed in incident

## References
- Git push.default documentation: https://git-scm.com/docs/git-config#Documentation/git-config.txt-pushdefault
- AGENTS.md § Prohibited Actions
- Incident: Codex force-pushed master via bare `git push --force-with-lease`

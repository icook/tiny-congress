# Documentation Index

This directory contains permanent, accepted documentation for TinyCongress.

## Directory Structure

| Directory | Purpose | Audience |
|-----------|---------|----------|
| [playbooks/](playbooks/) | Step-by-step how-to guides | Developers performing tasks |
| [interfaces/](interfaces/) | Contracts, schemas, naming rules | Developers writing new code |
| [decisions/](decisions/) | ADRs explaining architectural choices | Anyone questioning patterns |
| [checklists/](checklists/) | Pre-PR, pre-release, incident guides | Developers at checkpoints |
| [style/](style/) | UI styling guidelines | Frontend developers |
| [skills/](skills/) | LLM-specific task guides | AI assistants |

## Playbooks

How-to guides for common development tasks:

| Playbook | When to Use |
|----------|-------------|
| [local-dev-setup.md](playbooks/local-dev-setup.md) | Setting up development environment |
| [adding-migration.md](playbooks/adding-migration.md) | Making database schema changes |
| [new-graphql-endpoint.md](playbooks/new-graphql-endpoint.md) | Adding API endpoints |
| [debugging-ci-failure.md](playbooks/debugging-ci-failure.md) | Troubleshooting CI issues |
| [database-schema-change.md](playbooks/database-schema-change.md) | Database migrations workflow |
| [docker-layer-caching.md](playbooks/docker-layer-caching.md) | Optimizing Docker builds |
| [dependency-update.md](playbooks/dependency-update.md) | Updating project dependencies |
| [frontend-test-patterns.md](playbooks/frontend-test-patterns.md) | Writing frontend tests |
| [graphql-codegen.md](playbooks/graphql-codegen.md) | Generating GraphQL types |
| [pr-review-checklist.md](playbooks/pr-review-checklist.md) | Reviewing pull requests |
| [skaffold-profiles.md](playbooks/skaffold-profiles.md) | Using Skaffold profiles |

## Interfaces

Contracts and standards for consistency:

| Interface | Coverage |
|-----------|----------|
| [environment-variables.md](interfaces/environment-variables.md) | Required and optional env vars |
| [directory-conventions.md](interfaces/directory-conventions.md) | Where code lives |
| [naming-conventions.md](interfaces/naming-conventions.md) | How to name things |
| [branch-naming-conventions.md](interfaces/branch-naming-conventions.md) | Git branch naming standards |
| [api-contracts.md](interfaces/api-contracts.md) | API design patterns |
| [rust-coding-standards.md](interfaces/rust-coding-standards.md) | Rust style guide |
| [react-coding-standards.md](interfaces/react-coding-standards.md) | React/TypeScript patterns |
| [signed-envelope-spec.md](interfaces/signed-envelope-spec.md) | Cryptographic envelope format |

## Decisions (ADRs)

Architecture Decision Records explaining why we chose specific approaches:

| ADR | Decision |
|-----|----------|
| [001-cargo-chef-docker-builds.md](decisions/001-cargo-chef-docker-builds.md) | Using cargo-chef for Rust Docker builds |
| [002-skaffold-orchestration.md](decisions/002-skaffold-orchestration.md) | Skaffold for dev/CI orchestration |
| [003-pgmq-job-queue.md](decisions/003-pgmq-job-queue.md) | PostgreSQL-based job queue |
| [004-explicit-git-push-branches.md](decisions/004-explicit-git-push-branches.md) | Always specify branch on push |
| [005-mantine-first-styling.md](decisions/005-mantine-first-styling.md) | Mantine-first styling approach |

## Checklists

Pre-flight checks for critical operations:

- [pre-pr.md](checklists/pre-pr.md) - Before opening a PR
- [pre-release.md](checklists/pre-release.md) - Before deploying
- [incident.md](checklists/incident.md) - During incidents

## Style Guides

- [STYLE_GUIDE.md](style/STYLE_GUIDE.md) - Mantine-first styling policy
- [LLM_UI_GUIDE.md](style/LLM_UI_GUIDE.md) - LLM instructions for UI work

## Related Documentation

- [.plan/](../.plan/) - Ephemeral feature specs and tickets (removed on merge)
- [.scratch/](../.scratch/) - Temporary working notes (deleted after task)
- [CLAUDE.md](../CLAUDE.md) - AI assistant instructions and project rules

## Where Documentation Goes

| Document Type | Location | Lifecycle |
|---------------|----------|-----------|
| Permanent how-to guides | `docs/playbooks/` | Forever |
| Standards and contracts | `docs/interfaces/` | Forever |
| Architecture decisions | `docs/decisions/` | Forever |
| Pre-flight checklists | `docs/checklists/` | Forever |
| Styling guidelines | `docs/style/` | Forever |
| LLM task guides | `docs/skills/` | Forever |
| Feature specs in progress | `.plan/` | Removed on merge |
| Working notes and analysis | `.scratch/` | Deleted after task |

**Rule of thumb**: If it belongs in the repo permanently, it goes in `docs/`. If it's temporary working material, use `.plan/` or `.scratch/`.

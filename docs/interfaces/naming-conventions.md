# Naming Conventions

## Files

| Type | Convention | Example |
|------|------------|---------|
| Rust module | snake_case | `vote_handler.rs` |
| Rust test file | snake_case + `_tests` | `vote_handler_tests.rs` |
| React component | PascalCase | `VoteButton.tsx` |
| React component test | PascalCase + `.test` | `VoteButton.test.tsx` |
| React hook | camelCase + `use` prefix | `useVoting.ts` |
| React page | PascalCase | `Dashboard.tsx` |
| E2E test | kebab-case + `.spec` | `voting-flow.spec.ts` |
| SQL migration | `YYYYMMDDHHMMSS_description` | `20240115120000_add_votes_table.sql` |
| Kubernetes template | kebab-case | `deployment.yaml`, `hpa.yaml` |
| Shell script | kebab-case | `integration-coverage.sh` |
| Documentation | kebab-case | `adding-migration.md` |

## Code

### Rust

| Type | Convention | Example |
|------|------------|---------|
| Struct | PascalCase | `VoteResult` |
| Enum | PascalCase | `VoteStatus` |
| Function | snake_case | `count_votes()` |
| Variable | snake_case | `vote_count` |
| Constant | SCREAMING_SNAKE | `MAX_VOTES` |
| Module | snake_case | `mod vote_handler;` |
| Crate | kebab-case | `tiny-congress-api` |

### TypeScript/React

| Type | Convention | Example |
|------|------------|---------|
| Component | PascalCase | `VoteButton` |
| Hook | camelCase + use | `useVoting` |
| Function | camelCase | `countVotes()` |
| Variable | camelCase | `voteCount` |
| Constant | SCREAMING_SNAKE or camelCase | `MAX_VOTES`, `apiEndpoint` |
| Type/Interface | PascalCase | `VoteResult` |
| Enum | PascalCase | `VoteStatus` |
| CSS class | kebab-case | `.vote-button` |
| Test describe | Component/function name | `describe('VoteButton', ...)` |

### Database

| Type | Convention | Example |
|------|------------|---------|
| Table | snake_case, plural | `votes`, `user_sessions` |
| Column | snake_case | `created_at`, `user_id` |
| Index | `idx_table_columns` | `idx_votes_user_id` |
| Foreign key | `fk_table_ref` | `fk_votes_users` |
| Constraint | `chk_table_rule` | `chk_votes_positive` |

### Git

| Type | Convention | Example |
|------|------------|---------|
| Branch | `type/issue-slug` | `feature/123-add-voting` |
| Commit | Imperative, concise | `Add vote counting endpoint` |

Branch prefixes:
- `feature/` - New functionality
- `fix/` - Bug fixes
- `refactor/` - Code restructuring
- `docs/` - Documentation only
- `ci/` - CI/CD changes

## Anti-patterns

- ❌ `VoteButton_component.tsx` (redundant suffix)
- ❌ `handleVoteButtonClick` (verbose, use `handleClick` in context)
- ❌ `IVoteResult` (Hungarian notation for interfaces)
- ❌ `vote-handler.rs` (kebab-case in Rust)
- ❌ `Vote` table (singular)

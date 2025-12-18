# Database Migrations

SQL migration files for the TinyCongress database schema, managed by [sqlx](https://github.com/launchbadge/sqlx).

## Structure

Migrations are numbered sequentially and run in order:

| File | Purpose |
|------|---------|
| `01_init.sql` | Initial database setup |
| `02_test_table.sql` | Test infrastructure |
| `03_accounts.sql` | User account tables |

## Adding Migrations

See the [adding-migration playbook](../../docs/playbooks/adding-migration.md) for the full workflow.

Quick reference:
```bash
cd service
sqlx migrate add <descriptive_name>
# Edit the generated file
DATABASE_URL=postgres://postgres:postgres@localhost:5432/tiny-congress sqlx migrate run
cargo sqlx prepare
```

## Requirements

- Migrations require the pgmq extension (installed via `dockerfiles/Dockerfile.postgres`)
- The `.sqlx/` directory must be regenerated after migration changes (`cargo sqlx prepare`)

## Constraints

- DO NOT add new tables without explicit approval (see CLAUDE.md)
- DO NOT modify existing column types without a migration path

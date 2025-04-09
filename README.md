# TinyCongress

This is a non-functional WIP monorepo for a web community.

- `/web/` contains a React based UI for end user consumption
- `/service/` contains a Go REST/ws API consumed by the UI

# Dev

To update the sqlx validation:

```
docker-compose -f docker-compose.test.yml up -d
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/tinycongress
cargo install sqlx-cli
cargo sqlx migrate run
cargo sqlx prepare
```

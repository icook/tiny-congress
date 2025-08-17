# TinyCongress

This is a non-functional WIP monorepo for a web community.

## Core Components

- `/web/`
    - A simple React UI will offer allow users to create accounts, manage keys and participate in polling rooms. Mantine UI has been picked for the component library.
- `/service/`
    - A Rust based graphql API implements polling room runtime and CRUD endpoints. axum, tokio, and sqlx.

# Dev

[Skaffold](https://skaffold.dev/) manages:

- Local dev cluster with hot reload
- CI cluster setup test running
- Production image building

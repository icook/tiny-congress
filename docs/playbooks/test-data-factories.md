# Test Data Factories

This playbook documents the factory pattern for creating test data in backend tests.

## Overview

Test data factories provide a builder pattern for creating database entities with sensible defaults. This reduces test setup boilerplate and ensures consistent test data across the test suite.

## Available Factories

### AccountFactory

Creates user accounts with auto-generated usernames and key pairs.

```rust
use common::factories::AccountFactory;

// Create with all defaults
let account = AccountFactory::new().create(&mut tx).await;

// Customize username
let account = AccountFactory::new()
    .with_username("alice")
    .create(&mut tx).await;

// Customize key generation seed (for reproducible keys)
let account = AccountFactory::new()
    .with_seed(42)
    .create(&mut tx).await;

// Combine customizations
let account = AccountFactory::new()
    .with_username("bob")
    .with_seed(123)
    .create(&mut tx).await;
```

### TestItemFactory

Creates test items for basic CRUD testing.

```rust
use common::factories::TestItemFactory;

// Create with all defaults
let item = TestItemFactory::new().create(&mut tx).await;

// Customize name
let item = TestItemFactory::new()
    .with_name("special item")
    .create(&mut tx).await;
```

## Usage with Different Executors

Factories work with any sqlx executor type:

```rust
// With test transaction (rolled back after test)
let mut tx = test_transaction().await;
let account = AccountFactory::new().create(&mut *tx).await;

// With isolated database pool (persists within isolated db)
let db = isolated_db().await;
let account = AccountFactory::new().create(db.pool()).await;
```

## Adding New Factories

When adding a new entity, create a factory following this pattern:

1. Create `service/tests/common/factories/{entity}.rs`
2. Export from `service/tests/common/factories/mod.rs`
3. Use `next_id()` for generating unique default values
4. Implement builder methods returning `Self` for chaining
5. Implement `create()` that takes a generic executor

Template:

```rust
use super::next_id;

pub struct CreatedEntity {
    pub id: Uuid,
    // ... other fields
}

pub struct EntityFactory {
    field: Option<String>,
}

impl EntityFactory {
    pub fn new() -> Self {
        Self { field: None }
    }

    pub fn with_field(mut self, value: &str) -> Self {
        self.field = Some(value.to_string());
        self
    }

    pub async fn create<'e, E>(self, executor: E) -> CreatedEntity
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        let id = next_id();
        let field = self.field.unwrap_or_else(|| format!("entity_{id}"));
        // ... insert into database
    }
}

impl Default for EntityFactory {
    fn default() -> Self {
        Self::new()
    }
}
```

## Design Principles

1. **Sensible defaults**: Every field has a reasonable default value
2. **Unique values**: Use `next_id()` to generate unique identifiers
3. **Builder pattern**: Methods return `Self` for fluent chaining
4. **Executor agnostic**: Works with transactions, connections, and pools
5. **Panic on failure**: Factories panic on database errors (test failures are expected to be loud)

//! `TestItem` factory for test data creation.

use super::next_id;
use sqlx::query;
use uuid::Uuid;

/// Result of creating a test item.
#[derive(Debug, Clone)]
pub struct CreatedTestItem {
    pub id: Uuid,
    pub name: String,
}

/// Builder for creating test items with sensible defaults.
///
/// # Examples
///
/// ```rust
/// // Create with all defaults
/// let item = TestItemFactory::new().create(&mut tx).await;
///
/// // Customize the name
/// let item = TestItemFactory::new()
///     .with_name("special item")
///     .create(&mut tx).await;
/// ```
pub struct TestItemFactory {
    name: Option<String>,
}

impl TestItemFactory {
    /// Create a new factory with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self { name: None }
    }

    /// Set a specific name for the test item.
    #[must_use]
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }

    /// Create the test item in the database.
    ///
    /// # Panics
    ///
    /// Panics if the database insert fails.
    pub async fn create<'e, E>(self, executor: E) -> CreatedTestItem
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        let id = next_id();
        let name = self.name.unwrap_or_else(|| format!("test_item_{id}"));
        let uuid = Uuid::new_v4();

        query("INSERT INTO test_items (id, name) VALUES ($1, $2)")
            .bind(uuid)
            .bind(&name)
            .execute(executor)
            .await
            .expect("TestItemFactory: failed to create test item");

        CreatedTestItem { id: uuid, name }
    }
}

impl Default for TestItemFactory {
    fn default() -> Self {
        Self::new()
    }
}

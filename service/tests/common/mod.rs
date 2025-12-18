//! Common test utilities for integration tests.
//!
//! This module provides:
//!
//! - [`app_builder::TestAppBuilder`] - Build test Axum apps that mirror main.rs wiring
//! - [`test_db`] - Shared PostgreSQL container for database integration tests
//!
//! # App Builder Usage
//!
//! ```ignore
//! use crate::common::app_builder::TestAppBuilder;
//!
//! #[tokio::test]
//! async fn test_with_app() {
//!     let app = TestAppBuilder::with_mocks().build();
//!     // Use app.oneshot(...) to send requests
//! }
//! ```
//!
//! See [`app_builder`] module for preset builders and configuration options.
//!
//! # Database Test Usage
//!
//! Use `#[test]` (not `#[tokio::test]`) and wrap async code with `run_test`:
//!
//! ```ignore
//! use crate::common::test_db::{run_test, get_test_db};
//!
//! #[test]
//! fn test_something_with_db() {
//!     run_test(async {
//!         let db = get_test_db().await;
//!         // Use db.pool() for your test...
//!     });
//! }
//! ```
//!
//! # Why the shared runtime pattern?
//!
//! `#[tokio::test]` creates a runtime per test. When tests finish, async cleanup
//! may not complete before the runtime is destroyed, leaving "zombie" connections
//! that appear idle but have broken sockets. Using a shared runtime ensures all
//! async teardown completes properly.
//!
//! # Environment Variables
//!
//! - `TEST_POSTGRES_IMAGE`: Override the postgres image (default: `tc-postgres:local`)
//!   In CI, set to the GHCR image: `ghcr.io/icook/tiny-congress/postgres:$SHA`

pub mod app_builder;

pub mod test_db {
    use once_cell::sync::Lazy;
    use sqlx_core::migrate::Migrator;
    use sqlx_postgres::{PgPool, PgPoolOptions};
    use std::future::Future;
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Duration;
    use testcontainers::{runners::AsyncRunner, ContainerAsync, GenericImage, ImageExt};
    use tokio::runtime::Runtime;
    use tokio::sync::OnceCell;

    /// Global Tokio runtime shared across all tests.
    /// This ensures async cleanup happens while the runtime is still alive.
    static TEST_RUNTIME: Lazy<Runtime> = Lazy::new(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create test runtime")
    });

    /// Shared test database state - container + pool
    static TEST_DB: OnceCell<TestDb> = OnceCell::const_new();

    /// RAII guard holding both the pool and container.
    /// Container is kept alive as long as the pool exists.
    pub struct TestDb {
        pool: PgPool,
        _container: Arc<ContainerAsync<GenericImage>>,
    }

    impl TestDb {
        /// Get the connection pool
        pub fn pool(&self) -> &PgPool {
            &self.pool
        }
    }

    /// Run an async test on the shared runtime.
    /// Use this instead of `#[tokio::test]` to ensure proper async cleanup.
    pub fn run_test<F>(f: F)
    where
        F: Future<Output = ()>,
    {
        TEST_RUNTIME.block_on(f);
    }

    /// Get a reference to the shared test database.
    /// Initializes the container and pool on first call.
    #[allow(clippy::expect_used)]
    pub async fn get_test_db() -> &'static TestDb {
        TEST_DB
            .get_or_init(|| async {
                // Get image from env or use default local image
                let image_full = std::env::var("TEST_POSTGRES_IMAGE")
                    .unwrap_or_else(|_| "tc-postgres:local".to_string());

                // Parse image:tag format
                let (image_name, image_tag) = image_full
                    .rsplit_once(':')
                    .unwrap_or((&image_full, "latest"));

                // Start postgres container with custom image that includes pgmq
                let container = GenericImage::new(image_name, image_tag)
                    .with_exposed_port(5432.into())
                    .with_wait_for(testcontainers::core::WaitFor::message_on_stderr(
                        "database system is ready to accept connections",
                    ))
                    .with_env_var("POSTGRES_USER", "postgres")
                    .with_env_var("POSTGRES_PASSWORD", "postgres")
                    .with_env_var("POSTGRES_DB", "tiny-congress")
                    .start()
                    .await
                    .expect("Failed to start postgres container");

                let port = container
                    .get_host_port_ipv4(5432)
                    .await
                    .expect("Failed to get postgres port");

                // Wrap container in Arc to share ownership
                let container = Arc::new(container);

                // Build connection string
                let database_url =
                    format!("postgres://postgres:postgres@127.0.0.1:{port}/tiny-congress");

                // Connect to the database
                let pool = PgPoolOptions::new()
                    .max_connections(5)
                    .acquire_timeout(Duration::from_secs(30))
                    .connect(&database_url)
                    .await
                    .expect("Failed to connect to test database");

                // Run migrations
                let migrator = Migrator::new(Path::new(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/migrations"
                )))
                .await
                .expect("Failed to load migrations");

                migrator.run(&pool).await.expect("Failed to run migrations");

                // Verify pool is working
                sqlx_core::query_scalar::query_scalar::<_, i32>("SELECT 1")
                    .fetch_one(&pool)
                    .await
                    .expect("Failed to verify pool connectivity");

                TestDb {
                    pool,
                    _container: container,
                }
            })
            .await
    }
}

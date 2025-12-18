//! Common test utilities for database integration tests.
//!
//! This module provides a shared PostgreSQL container and proper async lifecycle
//! management to avoid "zombie connection" issues that occur when async cleanup
//! happens after the Tokio runtime is gone.
//!
//! # Usage
//!
//! Use `#[test]` (not `#[tokio::test]`) and wrap async code with `run_test`.
//! For isolated DB mutations, prefer a rollback-only transaction helper:
//!
//! ```ignore
//! use crate::common::test_db::{run_test, test_transaction};
//!
//! #[test]
//! fn test_something_with_db() {
//!     run_test(async {
//!         let mut tx = test_transaction().await;
//!         sqlx::query("INSERT ...").execute(&mut *tx).await.unwrap();
//!     });
//! }
//! ```
//!
//! # Why this pattern?
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

pub mod test_db {
    use once_cell::sync::Lazy;
    use sqlx::postgres::{PgConnection, PgPool, PgPoolOptions};
    use sqlx::Connection;
    use sqlx_core::migrate::Migrator;
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

    /// RAII guard that begins a transaction and rolls it back on drop.
    pub struct TestTransaction {
        conn: Option<PgConnection>,
    }

    impl TestTransaction {
        pub async fn new() -> Self {
            let db = get_test_db().await;
            let mut conn = PgConnection::connect(db.database_url())
                .await
                .expect("Failed to connect to test database");

            sqlx::query("BEGIN")
                .execute(&mut conn)
                .await
                .expect("Failed to start test transaction");

            Self { conn: Some(conn) }
        }
    }

    impl std::ops::Deref for TestTransaction {
        type Target = PgConnection;

        fn deref(&self) -> &Self::Target {
            self.conn.as_ref().expect("transaction missing connection")
        }
    }

    impl std::ops::DerefMut for TestTransaction {
        fn deref_mut(&mut self) -> &mut Self::Target {
            self.conn.as_mut().expect("transaction missing connection")
        }
    }

    impl Drop for TestTransaction {
        fn drop(&mut self) {
            if let Some(mut conn) = self.conn.take() {
                let _ = TEST_RUNTIME.spawn(async move {
                    let _ = sqlx::query("ROLLBACK").execute(&mut conn).await;
                });
            }
        }
    }

    /// Convenience helper to create a rollback-only transaction for a test.
    pub async fn test_transaction() -> TestTransaction {
        TestTransaction::new().await
    }

    /// RAII guard holding both the pool and container.
    /// Container is kept alive as long as the pool exists.
    pub struct TestDb {
        pool: PgPool,
        _container: Arc<ContainerAsync<GenericImage>>,
        database_url: String,
    }

    impl TestDb {
        /// Get the connection pool
        pub fn pool(&self) -> &PgPool {
            &self.pool
        }

        pub fn database_url(&self) -> &str {
            &self.database_url
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
                    database_url,
                }
            })
            .await
    }
}

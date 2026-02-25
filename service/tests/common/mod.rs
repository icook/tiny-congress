#![allow(unused)]
//! Common test utilities for integration tests.
//!
//! This module provides:
//!
//! - [`app_builder::TestAppBuilder`] - Build test Axum apps that mirror main.rs wiring
//! - [`test_db`] - Shared PostgreSQL container for database integration tests
//! - [`graphql`] - GraphQL response helpers for testing schema behavior
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
//! Use `#[shared_runtime_test]` from `tc-test-macros` for async database tests.
//! This runs tests on a shared Tokio runtime to ensure proper async cleanup.
//!
//! ## When to use each pattern:
//!
//! ### `test_transaction()` - 95% of DB tests (fast, simple)
//! - Query logic, CRUD operations, business logic
//! - Any test that doesn't need explicit transaction control
//! - Fast (~1-5ms setup) because it reuses the shared database
//!
//! ```ignore
//! use crate::common::test_db::test_transaction;
//! use tc_test_macros::shared_runtime_test;
//!
//! #[shared_runtime_test]
//! async fn test_something_with_db() {
//!     let mut tx = test_transaction().await;
//!     sqlx::query("INSERT ...").execute(&mut *tx).await.unwrap();
//!     // Transaction auto-rolls back on drop
//! }
//! ```
//!
//! ### `isolated_db()` - Specialized tests requiring full DB isolation
//! - Migration testing (rollback, idempotency)
//! - Concurrent transaction behavior (SELECT FOR UPDATE, isolation levels)
//! - Transaction isolation levels (SERIALIZABLE)
//! - Database-level features (LISTEN/NOTIFY, advisory locks)
//! - Testing explicit BEGIN/COMMIT/ROLLBACK logic
//! - Slower (~15-30ms setup) but provides complete isolation
//!
//! ```ignore
//! use crate::common::test_db::isolated_db;
//! use tc_test_macros::shared_runtime_test;
//!
//! #[shared_runtime_test]
//! async fn test_migration_idempotency() {
//!     let db = isolated_db().await;
//!     // This database is fully isolated - run migrations, test transactions, etc.
//!     // Database is automatically dropped when `db` goes out of scope
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
pub mod factories;
pub mod graphql;
pub mod migration_helpers;

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
        /// Port for connecting to the container
        port: u16,
    }

    impl TestDb {
        /// Get the connection pool
        pub fn pool(&self) -> &PgPool {
            &self.pool
        }

        pub fn database_url(&self) -> &str {
            &self.database_url
        }

        /// Get the port for the test container
        pub fn port(&self) -> u16 {
            self.port
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

                // Run migrations using a single connection (not a pool)
                // so we can close it and create the template before opening the pool
                let mut migration_conn = PgConnection::connect(&database_url)
                    .await
                    .expect("Failed to connect to test database for migrations");

                let migrator = Migrator::new(Path::new(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/migrations"
                )))
                .await
                .expect("Failed to load migrations");

                migrator
                    .run(&mut migration_conn)
                    .await
                    .expect("Failed to run migrations");

                // Close the migration connection so tiny-congress has no active sessions
                drop(migration_conn);

                // Create a template database for isolated_db() to use
                // We do this while no connections exist to tiny-congress
                let maintenance_url =
                    format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
                let mut maint_conn = PgConnection::connect(&maintenance_url)
                    .await
                    .expect("Failed to connect to postgres database for template creation");

                // Drop template if it exists (from previous test run)
                sqlx::query("DROP DATABASE IF EXISTS \"tiny_congress_template\"")
                    .execute(&mut maint_conn)
                    .await
                    .expect("Failed to drop old template");

                // Create template database from tiny-congress
                sqlx::query(
                    "CREATE DATABASE \"tiny_congress_template\" TEMPLATE \"tiny-congress\"",
                )
                .execute(&mut maint_conn)
                .await
                .expect("Failed to create template database");

                // Now create the pool for regular test usage
                let pool = PgPoolOptions::new()
                    .max_connections(5)
                    .acquire_timeout(Duration::from_secs(30))
                    .connect(&database_url)
                    .await
                    .expect("Failed to connect to test database");

                // Verify pool is working
                sqlx_core::query_scalar::query_scalar::<_, i32>("SELECT 1")
                    .fetch_one(&pool)
                    .await
                    .expect("Failed to verify pool connectivity");

                TestDb {
                    pool,
                    _container: container,
                    database_url,
                    port,
                }
            })
            .await
    }

    /// RAII guard for an isolated test database created via PostgreSQL template copy.
    ///
    /// This creates a unique database by copying from the shared test DB (which has
    /// migrations already applied). The database is automatically dropped when this
    /// struct is dropped.
    ///
    /// Use this for tests that need:
    /// - Full database isolation (not just transaction rollback)
    /// - Testing explicit transaction control (BEGIN/COMMIT/ROLLBACK)
    /// - Migration testing (rollback, idempotency)
    /// - Concurrent transaction behavior (SELECT FOR UPDATE, isolation levels)
    /// - Database-level features (LISTEN/NOTIFY, advisory locks)
    pub struct IsolatedDb {
        pool: PgPool,
        database_name: String,
        database_url: String,
        /// Port of the shared test container (used for cleanup connection)
        port: u16,
    }

    impl IsolatedDb {
        /// Get the connection pool for this isolated database.
        pub fn pool(&self) -> &PgPool {
            &self.pool
        }

        /// Get the database URL for this isolated database.
        pub fn database_url(&self) -> &str {
            &self.database_url
        }

        /// Get the database name.
        pub fn database_name(&self) -> &str {
            &self.database_name
        }
    }

    impl Drop for IsolatedDb {
        fn drop(&mut self) {
            let db_name = self.database_name.clone();
            let port = self.port;

            // Spawn cleanup on the shared runtime to ensure it completes
            TEST_RUNTIME.spawn(async move {
                // Connect to postgres (maintenance) database to perform cleanup
                let maintenance_url =
                    format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

                if let Ok(mut conn) = PgConnection::connect(&maintenance_url).await {
                    // Terminate any remaining connections to the isolated database
                    let _ = sqlx::query(&format!(
                        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{db_name}'"
                    ))
                    .execute(&mut conn)
                    .await;

                    // Drop the database
                    let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS \"{db_name}\""))
                        .execute(&mut conn)
                        .await;
                }
            });
        }
    }

    /// Create an isolated test database via PostgreSQL template copy.
    ///
    /// This is ~10x faster than re-running migrations for each test because
    /// PostgreSQL performs a filesystem-level copy of the template database.
    ///
    /// # Performance
    /// - Template copy: ~15-30ms (current schema)
    /// - Migration re-run: ~100-300ms (current schema)
    ///
    /// # Example
    /// ```ignore
    /// use crate::common::test_db::{run_test, isolated_db};
    ///
    /// #[test]
    /// fn test_migration_idempotency() {
    ///     run_test(async {
    ///         let db = isolated_db().await;
    ///         // This database is fully isolated - run migrations, test transactions, etc.
    ///     });
    /// }
    /// ```
    #[allow(clippy::expect_used)]
    pub async fn isolated_db() -> IsolatedDb {
        // Ensure shared test DB is initialized (this runs migrations and creates template)
        let test_db = get_test_db().await;
        let port = test_db.port();

        // Generate unique database name
        let db_name = format!("test_isolated_{}", uuid::Uuid::new_v4().simple());

        // Connect to postgres (maintenance) database to create the isolated DB
        let maintenance_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
        let mut maint_conn = PgConnection::connect(&maintenance_url)
            .await
            .expect("Failed to connect to postgres database");

        // Create the isolated database using the template database
        // We use tiny_congress_template which was created during get_test_db()
        // and has no active connections (unlike the main tiny-congress database)
        sqlx::query(&format!(
            "CREATE DATABASE \"{db_name}\" TEMPLATE \"tiny_congress_template\""
        ))
        .execute(&mut maint_conn)
        .await
        .expect("Failed to create isolated database from template");

        // Build connection string for the new database
        let database_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/{db_name}");

        // Connect to the new isolated database
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(30))
            .connect(&database_url)
            .await
            .expect("Failed to connect to isolated database");

        IsolatedDb {
            pool,
            database_name: db_name,
            database_url,
            port,
        }
    }

    /// Create a truly empty test database with no migrations applied.
    ///
    /// Unlike `isolated_db()` which copies from a template with migrations already
    /// applied, this creates a completely empty database. Use this for tests that
    /// need to:
    /// - Verify migrations apply correctly from scratch
    /// - Test migration idempotency by running migrations multiple times
    ///
    /// # Performance
    /// - Empty DB creation: ~5-10ms
    /// - Migrations must be run manually after creation
    ///
    /// # Example
    /// ```ignore
    /// use crate::common::test_db::empty_db;
    /// use crate::common::migration_helpers::load_migrator;
    ///
    /// #[shared_runtime_test]
    /// async fn test_migrations_from_scratch() {
    ///     let db = empty_db().await;
    ///     let migrator = load_migrator().await;
    ///     migrator.run(db.pool()).await.unwrap();
    ///     // Now test the result
    /// }
    /// ```
    #[allow(clippy::expect_used)]
    pub async fn empty_db() -> IsolatedDb {
        // Ensure shared test DB is initialized (we need the container port)
        let test_db = get_test_db().await;
        let port = test_db.port();

        // Generate unique database name
        let db_name = format!("test_empty_{}", uuid::Uuid::new_v4().simple());

        // Connect to postgres (maintenance) database to create the empty DB
        let maintenance_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
        let mut maint_conn = PgConnection::connect(&maintenance_url)
            .await
            .expect("Failed to connect to postgres database");

        // Create a completely empty database (no template)
        sqlx::query(&format!("CREATE DATABASE \"{db_name}\""))
            .execute(&mut maint_conn)
            .await
            .expect("Failed to create empty database");

        // Build connection string for the new database
        let database_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/{db_name}");

        // Connect to the new empty database
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(30))
            .connect(&database_url)
            .await
            .expect("Failed to connect to empty database");

        IsolatedDb {
            pool,
            database_name: db_name,
            database_url,
            port,
        }
    }
}

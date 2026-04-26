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
//! ## Container Sharing
//!
//! A single Postgres container is shared across all test binaries in a
//! `cargo test` run via a state file at `/tmp/tc-test-postgres.json`.
//! The first binary to start creates the container; subsequent binaries
//! reuse it via TCP health probe. This reduces container count from ~24
//! to 1 during a full test run.
//!
//! The state file also stores a SHA-256 hash of all migration `.sql` files.
//! When any migration changes (rename, edit, add new), the hash no longer
//! matches and the `tiny_congress_template` database is automatically dropped
//! and rebuilt on the next test run — no manual `docker rm` required.
//!
//! To force a fresh container (e.g., after changing the postgres image):
//! ```bash
//! rm /tmp/tc-test-postgres.json
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
//! - `/tmp/tc-test-postgres.json`: Shared container state file. Delete to force a fresh container.

pub mod app_builder;
pub mod factories;
pub mod graphql;
pub mod migration_helpers;
pub mod simulation;

pub mod test_db {
    use sqlx::postgres::{PgConnection, PgPool, PgPoolOptions};
    use sqlx::Connection;
    use sqlx_core::migrate::Migrator;
    use std::future::Future;
    use std::io::{Read as _, Write as _};
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::LazyLock;
    use std::time::Duration;
    use testcontainers::{runners::AsyncRunner, ContainerAsync, GenericImage, ImageExt};
    use tokio::runtime::Runtime;
    use tokio::sync::OnceCell;

    const STATE_FILE: &str = "/tmp/tc-test-postgres.json";
    const LOCK_FILE: &str = "/tmp/tc-test-postgres.lock";

    /// Connection info for a shared test container, persisted to disk
    /// so multiple test binaries can reuse the same container.
    #[derive(serde::Serialize, serde::Deserialize)]
    struct SharedContainerInfo {
        container_id: String,
        host: String,
        port: u16,
        /// SHA-256 hex digest of all migration .sql files (sorted by name).
        /// When this changes, the template database is invalidated and rebuilt.
        #[serde(default)]
        migration_hash: String,
    }

    /// Compute a SHA-256 hash over all migration `.sql` files (sorted by name).
    /// Returns a lowercase hex string, or an empty string on any I/O error.
    fn compute_migration_hash() -> String {
        use sha2::{Digest, Sha256};

        let migrations_dir =
            std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations"));

        let mut entries: Vec<_> = match std::fs::read_dir(migrations_dir) {
            Ok(rd) => rd
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|x| x == "sql").unwrap_or(false))
                .collect(),
            Err(_) => return String::new(),
        };

        entries.sort_by_key(|e| e.file_name());

        let mut hasher = Sha256::new();
        for entry in entries {
            match std::fs::read(entry.path()) {
                Ok(contents) => {
                    // Include the filename in the hash so renames are detected.
                    hasher.update(entry.file_name().to_string_lossy().as_bytes());
                    hasher.update(b"\0");
                    hasher.update(&contents);
                    hasher.update(b"\0");
                }
                Err(_) => return String::new(),
            }
        }

        format!("{:x}", hasher.finalize())
    }

    /// RAII file lock using flock(2). Holds an exclusive lock on LOCK_FILE
    /// for the duration of container init — prevents two binaries from
    /// racing to start containers simultaneously.
    struct FileLock {
        file: std::fs::File,
    }

    impl FileLock {
        fn acquire() -> Self {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(false)
                .open(LOCK_FILE)
                .expect("Failed to open lock file");

            // SAFETY: flock on a valid fd is safe. LOCK_EX blocks until acquired.
            unsafe {
                if libc::flock(std::os::unix::io::AsRawFd::as_raw_fd(&file), libc::LOCK_EX) != 0 {
                    panic!("flock failed: {}", std::io::Error::last_os_error());
                }
            }

            Self { file }
        }
    }

    impl Drop for FileLock {
        fn drop(&mut self) {
            // SAFETY: flock on a valid fd is safe. LOCK_UN never blocks.
            unsafe {
                libc::flock(
                    std::os::unix::io::AsRawFd::as_raw_fd(&self.file),
                    libc::LOCK_UN,
                );
            }
        }
    }

    /// Check if a container is alive by attempting a TCP connection.
    /// Returns true if any resolved address accepts a connection.
    fn is_container_alive(host: &str, port: u16) -> bool {
        use std::net::ToSocketAddrs;
        // Resolve hostname first — `connect_timeout` requires a `SocketAddr`,
        // not a hostname string. Try all resolved addresses because `localhost`
        // resolves to `::1` first on macOS, but Docker only binds IPv4 ports.
        let addr_str = format!("{host}:{port}");
        let Ok(addrs) = addr_str.to_socket_addrs() else {
            return false;
        };
        addrs
            .into_iter()
            .any(|addr| std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(2)).is_ok())
    }

    fn read_state_file() -> Option<SharedContainerInfo> {
        let data = std::fs::read_to_string(STATE_FILE).ok()?;
        serde_json::from_str(&data).ok()
    }

    fn write_state_file(info: &SharedContainerInfo) {
        let data = serde_json::to_string_pretty(info).expect("Failed to serialize container state");
        std::fs::write(STATE_FILE, data).expect("Failed to write container state file");
    }

    /// Global Tokio runtime shared across all tests.
    /// This ensures async cleanup happens while the runtime is still alive.
    static TEST_RUNTIME: LazyLock<Runtime> = LazyLock::new(|| {
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
        /// Holds the container handle when this process started it.
        /// None when reusing a container started by another process.
        _container: Option<Arc<ContainerAsync<GenericImage>>>,
        database_url: String,
        /// Host for connecting to the container (localhost or remote Docker host)
        host: String,
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

        /// Get the host for the test container
        pub fn host(&self) -> &str {
            &self.host
        }

        /// Get the port for the test container
        pub fn port(&self) -> u16 {
            self.port
        }
    }

    /// Run an async test on the shared runtime.
    /// Use this instead of `#[tokio::test]` to ensure proper async cleanup.
    pub fn run_test<F>(f: F) -> F::Output
    where
        F: Future,
    {
        TEST_RUNTIME.block_on(f)
    }

    /// Parse host, port, user, and password from a postgres://user:pass@host:port/db URL.
    fn parse_postgres_url(url: &str) -> (String, u16, String, String, String) {
        let stripped = url
            .strip_prefix("postgres://")
            .or_else(|| url.strip_prefix("postgresql://"))
            .expect("TEST_DATABASE_URL must start with postgres://");
        let (userinfo, rest) = stripped.split_once('@').expect("missing @ in URL");
        let (user, password) = userinfo.split_once(':').unwrap_or((userinfo, ""));
        let (hostport, dbname) = rest.split_once('/').unwrap_or((rest, ""));
        let (host, port_str) = hostport.split_once(':').unwrap_or((hostport, "5432"));
        let port: u16 = port_str.parse().expect("invalid port in URL");
        (
            host.to_string(),
            port,
            user.to_string(),
            password.to_string(),
            dbname.to_string(),
        )
    }

    /// Get a reference to the shared test database.
    /// Initializes the container and pool on first call.
    #[allow(clippy::expect_used)]
    pub async fn get_test_db() -> &'static TestDb {
        TEST_DB
            .get_or_init(|| async {
                // If TEST_DATABASE_URL is set, use the external postgres directly
                // (e.g., GitLab CI services: sidecar). Skip container startup entirely.
                if let Ok(ext_url) = std::env::var("TEST_DATABASE_URL") {
                    let (host, port, user, password, _dbname) = parse_postgres_url(&ext_url);

                    // Use the provided database for migrations
                    let database_url = ext_url.clone();
                    let mut migration_conn = PgConnection::connect(&database_url)
                        .await
                        .expect("Failed to connect to TEST_DATABASE_URL");

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

                    sqlx::query(
                        "CREATE TABLE IF NOT EXISTS test_items (
                            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                            name TEXT NOT NULL,
                            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
                        )",
                    )
                    .execute(&mut migration_conn)
                    .await
                    .expect("Failed to create test_items table");

                    drop(migration_conn);

                    // Create template database for isolated_db()
                    let maintenance_url =
                        format!("postgres://{user}:{password}@{host}:{port}/postgres");
                    let mut maint_conn = PgConnection::connect(&maintenance_url)
                        .await
                        .expect("Failed to connect to postgres for template check");

                    let template_exists: bool = sqlx::query_scalar(
                        "SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = 'tiny_congress_template')",
                    )
                    .fetch_one(&mut maint_conn)
                    .await
                    .expect("Failed to check template existence");

                    if !template_exists {
                        let _ = sqlx::query(
                            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity \
                             WHERE datname = 'tiny_congress_test' AND pid != pg_backend_pid()",
                        )
                        .execute(&mut maint_conn)
                        .await;

                        sqlx::query(
                            "CREATE DATABASE \"tiny_congress_template\" TEMPLATE \"tiny_congress_test\"",
                        )
                        .execute(&mut maint_conn)
                        .await
                        .expect("Failed to create template database");
                    }

                    drop(maint_conn);

                    let pool = PgPoolOptions::new()
                        .max_connections(5)
                        .acquire_timeout(Duration::from_secs(30))
                        .connect(&database_url)
                        .await
                        .expect("Failed to connect to test database");

                    sqlx_core::query_scalar::query_scalar::<_, i32>("SELECT 1")
                        .fetch_one(&pool)
                        .await
                        .expect("Failed to verify pool connectivity");

                    return TestDb {
                        pool,
                        _container: None,
                        database_url,
                        host,
                        port,
                    };
                }

                // Phase 1: Acquire lock and determine if we need a new container.
                // Lock is released at the end of this block.
                let (host, port, container) = {
                    let _lock = FileLock::acquire();

                    if let Some(mut info) = read_state_file() {
                        if is_container_alive(&info.host, info.port) {
                            // Container exists and is healthy.
                            // Check whether migrations have changed since the
                            // template was last built. If so, drop and rebuild.
                            let current_hash = compute_migration_hash();
                            if !current_hash.is_empty() && current_hash != info.migration_hash {
                                eprintln!(
                                    "[test-db] Migration hash changed ({} → {}); \
                                     invalidating template database.",
                                    &info.migration_hash[..8.min(info.migration_hash.len())],
                                    &current_hash[..8],
                                );
                                let maintenance_url = format!(
                                    "postgres://postgres:postgres@{}:{}/postgres",
                                    info.host, info.port
                                );
                                // Connect synchronously is not possible here; use a blocking
                                // runtime call. We are already inside an async block on
                                // TEST_RUNTIME so we can await directly.
                                if let Ok(mut conn) =
                                    PgConnection::connect(&maintenance_url).await
                                {
                                    let _ = sqlx::query(
                                        "DROP DATABASE IF EXISTS tiny_congress_template",
                                    )
                                    .execute(&mut conn)
                                    .await;

                                    // Also reset the main DB so migrations re-run cleanly.
                                    let _ = sqlx::query(
                                        "SELECT pg_terminate_backend(pid) \
                                         FROM pg_stat_activity \
                                         WHERE datname = 'tiny-congress' \
                                           AND pid != pg_backend_pid()",
                                    )
                                    .execute(&mut conn)
                                    .await;

                                    let _ = sqlx::query(
                                        "DROP DATABASE IF EXISTS \"tiny-congress\"",
                                    )
                                    .execute(&mut conn)
                                    .await;

                                    let _ = sqlx::query(
                                        "CREATE DATABASE \"tiny-congress\"",
                                    )
                                    .execute(&mut conn)
                                    .await;
                                }

                                // Persist updated hash so the next binary skips this work.
                                info.migration_hash = current_hash;
                                write_state_file(&info);
                            }
                            (info.host, info.port, None)
                        } else {
                            // Stale state file — start a fresh container.
                            start_and_register_container().await
                        }
                    } else {
                        // No state file — start a fresh container.
                        start_and_register_container().await
                    }
                    // _lock dropped here. The lock is intentionally held during container
                    // start to prevent concurrent test binaries from racing to create
                    // duplicate containers. Migrations (Phase 2) run after the lock releases.
                };

                // Phase 2: Ensure migrations and template exist.
                // These operations are idempotent — safe to run even if another
                // binary already completed them.
                let database_url =
                    format!("postgres://postgres:postgres@{host}:{port}/tiny-congress");

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

                sqlx::query(
                    "CREATE TABLE IF NOT EXISTS test_items (
                        id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                        name TEXT NOT NULL,
                        created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
                    )",
                )
                .execute(&mut migration_conn)
                .await
                .expect("Failed to create test_items table");

                drop(migration_conn);

                // Create template if it does not already exist.
                let maintenance_url =
                    format!("postgres://postgres:postgres@{host}:{port}/postgres");
                let mut maint_conn = PgConnection::connect(&maintenance_url)
                    .await
                    .expect("Failed to connect to postgres database for template check");

                let template_exists: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = 'tiny_congress_template')",
                )
                .fetch_one(&mut maint_conn)
                .await
                .expect("Failed to check template existence");

                if !template_exists {
                    // Terminate all connections to tiny-congress so Postgres allows
                    // it to be used as a TEMPLATE source.
                    let _ = sqlx::query(
                        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity \
                         WHERE datname = 'tiny-congress' AND pid != pg_backend_pid()",
                    )
                    .execute(&mut maint_conn)
                    .await;

                    sqlx::query(
                        "CREATE DATABASE \"tiny_congress_template\" TEMPLATE \"tiny-congress\"",
                    )
                    .execute(&mut maint_conn)
                    .await
                    .expect("Failed to create template database");
                }

                drop(maint_conn);

                // Phase 3: Create pool for test usage.
                let pool = PgPoolOptions::new()
                    .max_connections(5)
                    .acquire_timeout(Duration::from_secs(30))
                    .connect(&database_url)
                    .await
                    .expect("Failed to connect to test database");

                sqlx_core::query_scalar::query_scalar::<_, i32>("SELECT 1")
                    .fetch_one(&pool)
                    .await
                    .expect("Failed to verify pool connectivity");

                TestDb {
                    pool,
                    _container: container,
                    database_url,
                    host,
                    port,
                }
            })
            .await
    }

    /// Start a new Postgres container and write its connection info to the state file.
    ///
    /// Must only be called while `FileLock` is held, so that concurrent test
    /// binaries do not race to start duplicate containers.
    async fn start_and_register_container(
    ) -> (String, u16, Option<Arc<ContainerAsync<GenericImage>>>) {
        let image_full = std::env::var("TEST_POSTGRES_IMAGE")
            .unwrap_or_else(|_| "tc-postgres:local".to_string());

        let (image_name, image_tag) = image_full
            .rsplit_once(':')
            .unwrap_or((&image_full, "latest"));

        let owner_pid = std::process::id().to_string();
        let container = GenericImage::new(image_name, image_tag)
            .with_exposed_port(5432.into())
            .with_wait_for(testcontainers::core::WaitFor::message_on_stderr(
                "database system is ready to accept connections",
            ))
            .with_env_var("POSTGRES_USER", "postgres")
            .with_env_var("POSTGRES_PASSWORD", "postgres")
            .with_env_var("POSTGRES_DB", "tiny-congress")
            .with_label("tc-owner-pid", owner_pid)
            .start()
            .await
            .expect("Failed to start postgres container");

        let host = container
            .get_host()
            .await
            .expect("Failed to get container host")
            .to_string();

        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("Failed to get postgres port");

        let container = Arc::new(container);

        write_state_file(&SharedContainerInfo {
            container_id: container.id().to_string(),
            host: host.clone(),
            port,
            // Record the hash at container-start time so the first binary that
            // runs migrations won't immediately re-invalidate the template.
            migration_hash: compute_migration_hash(),
        });

        (host, port, Some(container))
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
        /// Maintenance URL (postgres://user:pass@host:port/postgres) for cleanup
        maintenance_url: String,
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
            let maintenance_url = self.maintenance_url.clone();

            // Spawn cleanup on the shared runtime to ensure it completes
            TEST_RUNTIME.spawn(async move {
                // Connect to postgres (maintenance) database to perform cleanup
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
        let host = test_db.host();
        let port = test_db.port();

        // Generate unique database name
        let db_name = format!("test_isolated_{}", uuid::Uuid::new_v4().simple());

        // Connect to postgres (maintenance) database to create the isolated DB
        let maintenance_url = format!("postgres://postgres:postgres@{host}:{port}/postgres");
        // If using external DB, credentials may differ — override from env
        let maintenance_url = if std::env::var("TEST_DATABASE_URL").is_ok() {
            let (h, p, u, pw, _) = parse_postgres_url(&std::env::var("TEST_DATABASE_URL").unwrap());
            format!("postgres://{u}:{pw}@{h}:{p}/postgres")
        } else {
            maintenance_url
        };
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
        let database_url = if std::env::var("TEST_DATABASE_URL").is_ok() {
            let (h, p, u, pw, _) = parse_postgres_url(&std::env::var("TEST_DATABASE_URL").unwrap());
            format!("postgres://{u}:{pw}@{h}:{p}/{db_name}")
        } else {
            format!("postgres://postgres:postgres@{host}:{port}/{db_name}")
        };

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
            maintenance_url,
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
        let host = test_db.host();
        let port = test_db.port();

        // Generate unique database name
        let db_name = format!("test_empty_{}", uuid::Uuid::new_v4().simple());

        // Connect to postgres (maintenance) database to create the empty DB
        let maintenance_url = if let Ok(ext_url) = std::env::var("TEST_DATABASE_URL") {
            let (h, p, u, pw, _) = parse_postgres_url(&ext_url);
            format!("postgres://{u}:{pw}@{h}:{p}/postgres")
        } else {
            format!("postgres://postgres:postgres@{host}:{port}/postgres")
        };
        let mut maint_conn = PgConnection::connect(&maintenance_url)
            .await
            .expect("Failed to connect to postgres database");

        // Create a completely empty database (no template)
        sqlx::query(&format!("CREATE DATABASE \"{db_name}\""))
            .execute(&mut maint_conn)
            .await
            .expect("Failed to create empty database");

        // Build connection string for the new database
        let database_url = if let Ok(ext_url) = std::env::var("TEST_DATABASE_URL") {
            let (h, p, u, pw, _) = parse_postgres_url(&ext_url);
            format!("postgres://{u}:{pw}@{h}:{p}/{db_name}")
        } else {
            format!("postgres://postgres:postgres@{host}:{port}/{db_name}")
        };

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
            maintenance_url,
        }
    }
}

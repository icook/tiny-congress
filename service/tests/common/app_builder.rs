//! Test app builder that mirrors main.rs wiring with injectable deps/mocks.
//!
//! This module provides a [`TestAppBuilder`] that constructs an Axum router matching
//! the production configuration in `main.rs`, but with the ability to inject mocks
//! and test-specific configurations.
//!
//! # Usage
//!
//! ```ignore
//! use crate::common::app_builder::TestAppBuilder;
//!
//! #[tokio::test]
//! async fn test_with_full_app() {
//!     let app = TestAppBuilder::new()
//!         .with_graphql()
//!         .with_identity_lazy()
//!         .with_cors(&["http://localhost:3000"])
//!         .build();
//!
//!     // Use app.oneshot(...) to send requests
//! }
//! ```
//!
//! # Preset Builders
//!
//! - [`TestAppBuilder::minimal()`] - Health check only
//! - [`TestAppBuilder::graphql_only()`] - GraphQL without identity/CORS
//! - [`TestAppBuilder::with_mocks()`] - Full app with lazy pool (no real DB)

use std::sync::Arc;

use async_graphql::{EmptySubscription, Schema};
use axum::{
    http::{header::HeaderValue, Method, StatusCode},
    middleware,
    response::IntoResponse,
    routing::get,
    Extension, Router,
};
use sqlx::PgPool;
use tc_engine_api::{
    constraints::ConstraintRegistry,
    engine::{EngineContext, EngineRegistry},
};
use tc_engine_polling::engine::PollingEngine;
use tc_engine_polling::service::{DefaultPollingService, PollingService};
use tinycongress_api::{
    build_info::BuildInfo,
    config::SecurityHeadersConfig,
    graphql::{graphql_handler, graphql_playground, MutationRoot, QueryRoot},
    http::{build_security_headers, security_headers_middleware},
    identity::{
        self,
        http::backup::SyntheticBackupKey,
        repo::{IdentityRepo, PgIdentityRepo},
        service::{DefaultIdentityService, IdentityService},
    },
    reputation::{
        self,
        repo::{PgReputationRepo, ReputationRepo},
        service::{DefaultEndorsementService, EndorsementService},
    },
    rest::{self, ApiDoc},
    rooms::{
        self,
        content_filter::{ContentFilter, NoopFilter},
        repo::{PgRoomsRepo, RoomsRepo},
        service::{DefaultRoomsService, RoomsService},
    },
    trust::{
        self,
        graph_reader::TrustRepoGraphReader,
        repo::{PgTrustRepo, TrustRepo},
        service::{DefaultTrustService, TrustService},
    },
};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

/// Minimal [`RoomLifecycle`] stub for test wiring — does not hit the database.
struct StubRoomLifecycle;

#[async_trait::async_trait]
impl tc_engine_api::engine::RoomLifecycle for StubRoomLifecycle {
    async fn get_room(
        &self,
        _room_id: uuid::Uuid,
    ) -> Result<tc_engine_api::engine::RoomRecord, anyhow::Error> {
        anyhow::bail!("StubRoomLifecycle::get_room not implemented in tests")
    }

    async fn close_room(&self, _room_id: uuid::Uuid) -> Result<(), anyhow::Error> {
        anyhow::bail!("StubRoomLifecycle::close_room not implemented in tests")
    }
}

/// Liveness check handler (mirrors main.rs). Always returns 200.
async fn health_check() -> impl IntoResponse {
    StatusCode::OK
}

/// Readiness check handler (mirrors main.rs).
///
/// Returns 200 if the DB pool can acquire a connection, 503 otherwise.
/// When no pool is registered, returns 503.
async fn readiness_check(pool: Option<Extension<PgPool>>) -> impl IntoResponse {
    let Some(Extension(pool)) = pool else {
        return StatusCode::SERVICE_UNAVAILABLE;
    };
    match tokio::time::timeout(std::time::Duration::from_secs(2), pool.acquire()).await {
        Ok(Ok(_)) => StatusCode::OK,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    }
}

/// Builder for test applications that mirrors main.rs wiring.
///
/// Use the builder pattern to construct an Axum router with the exact same
/// layer ordering and configuration as production, while allowing injection
/// of mocks for testing.
pub struct TestAppBuilder {
    /// Whether to include GraphQL routes
    include_graphql: bool,
    /// Whether to include REST API routes
    include_rest: bool,
    /// Whether to include identity routes
    include_identity: bool,
    /// Whether to include reputation routes
    include_reputation: bool,
    /// Whether to include rooms routes
    include_rooms: bool,
    /// Whether to include trust routes
    include_trust: bool,
    /// Whether to include health check route
    include_health: bool,
    /// Whether to include Swagger UI
    include_swagger: bool,
    /// Custom build info provider (None uses from_env())
    build_info: Option<BuildInfo>,
    /// Database pool — only set by `with_identity_pool()` for integration tests
    /// that need the pool injected into the GraphQL schema.
    pool: Option<PgPool>,
    /// Identity service for identity routes
    identity_service: Option<Arc<dyn IdentityService>>,
    /// Identity repo for device/backup/login handlers
    identity_repo: Option<Arc<dyn IdentityRepo>>,
    /// Endorsement service for reputation + rooms
    endorsement_service: Option<Arc<dyn EndorsementService>>,
    /// Reputation repo for reputation routes
    reputation_repo: Option<Arc<dyn ReputationRepo>>,
    /// Rooms service for room CRUD routes
    rooms_service: Option<Arc<dyn RoomsService>>,
    /// Polling service for poll/vote/dimension routes
    polling_service: Option<Arc<dyn PollingService>>,
    /// Trust service for trust routes
    trust_service: Option<Arc<dyn TrustService>>,
    /// Trust repo for trust routes
    trust_repo: Option<Arc<dyn TrustRepo>>,
    /// Engine registry for room engine dispatch
    engine_registry: Option<Arc<EngineRegistry>>,
    /// Engine context for room engine hooks
    engine_ctx: Option<EngineContext>,
    /// Content filter for suggestion endpoints (None means no filter extension added)
    content_filter: Option<Arc<dyn ContentFilter>>,
    /// CORS allowed origins (None means no CORS layer)
    cors_origins: Option<Vec<String>>,
    /// Security headers config (None means disabled)
    security_headers: Option<SecurityHeadersConfig>,
}

impl Default for TestAppBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TestAppBuilder {
    /// Create a new empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            include_graphql: false,
            include_rest: false,
            include_identity: false,
            include_reputation: false,
            include_rooms: false,
            include_trust: false,
            include_health: false,
            include_swagger: false,
            build_info: None,
            pool: None,
            identity_service: None,
            identity_repo: None,
            endorsement_service: None,
            reputation_repo: None,
            rooms_service: None,
            polling_service: None,
            trust_service: None,
            trust_repo: None,
            engine_registry: None,
            engine_ctx: None,
            content_filter: None,
            cors_origins: None,
            security_headers: None,
        }
    }

    // =========================================================================
    // Preset Builders
    // =========================================================================

    /// Create a minimal app with only the health check endpoint.
    ///
    /// Use this for simple connectivity tests.
    #[must_use]
    pub fn minimal() -> Self {
        Self::new().with_health()
    }

    /// Create an app with GraphQL routes only.
    ///
    /// Includes GraphQL endpoint with build info but no identity,
    /// CORS, or security headers.
    #[must_use]
    pub fn graphql_only() -> Self {
        Self::new().with_graphql().with_health()
    }

    /// Create a full app with mock persistence (no real DB).
    ///
    /// Mirrors production main.rs wiring but with a mock repo instead
    /// of a real database connection. Includes all routes, CORS, and
    /// security headers. Identity routes run real validation through
    /// [`DefaultIdentityService`]; DB-dependent tests belong in
    /// identity_handler_tests.rs.
    #[must_use]
    pub fn with_mocks() -> Self {
        Self::new()
            .with_graphql()
            .with_rest()
            .with_identity_lazy()
            .with_health()
            .with_swagger()
            .with_cors(&["http://localhost:3000"])
            .with_security_headers_default()
    }

    // =========================================================================
    // Component Configuration
    // =========================================================================

    /// Include GraphQL routes (/graphql).
    #[must_use]
    pub fn with_graphql(mut self) -> Self {
        self.include_graphql = true;
        self
    }

    /// Include REST API routes (/api/v1/*).
    #[must_use]
    pub fn with_rest(mut self) -> Self {
        self.include_rest = true;
        self
    }

    /// Include identity routes with a real service backed by a mock repo (no DB needed).
    ///
    /// Uses [`DefaultIdentityService`] so request validation runs exactly as in
    /// production.  The underlying repo is a [`MockIdentityRepo`] so persistence
    /// calls succeed without a database.  Tests that need real DB behaviour
    /// (duplicate constraints, transactions) belong in identity_handler_tests.rs.
    #[must_use]
    pub fn with_identity_lazy(mut self) -> Self {
        use tinycongress_api::identity::repo::mock::MockIdentityRepo;
        self.include_identity = true;
        let repo = Arc::new(MockIdentityRepo::default());
        self.identity_repo = Some(Arc::clone(&repo) as Arc<dyn IdentityRepo>);
        self.identity_service =
            Some(Arc::new(DefaultIdentityService::new(repo)) as Arc<dyn IdentityService>);
        self
    }

    /// Include identity routes with a real database pool.
    #[must_use]
    pub fn with_identity_pool(mut self, pool: PgPool) -> Self {
        self.include_identity = true;
        let repo = Arc::new(PgIdentityRepo::new(pool.clone()));
        self.identity_repo = Some(Arc::clone(&repo) as Arc<dyn IdentityRepo>);
        self.identity_service =
            Some(Arc::new(DefaultIdentityService::new(repo)) as Arc<dyn IdentityService>);
        self.pool = Some(pool);
        self
    }

    /// Include rooms and reputation routes with a real database pool.
    ///
    /// This wires up the full rooms + polling + endorsement stack, matching main.rs.
    /// Identity routes are also enabled since room operations require auth.
    #[must_use]
    pub fn with_rooms_pool(mut self, pool: PgPool) -> Self {
        // Identity wiring (needed for auth)
        self.include_identity = true;
        let identity_repo = Arc::new(PgIdentityRepo::new(pool.clone()));
        self.identity_repo = Some(Arc::clone(&identity_repo) as Arc<dyn IdentityRepo>);
        self.identity_service =
            Some(Arc::new(DefaultIdentityService::new(identity_repo)) as Arc<dyn IdentityService>);

        // Reputation wiring
        self.include_reputation = true;
        let reputation_repo = Arc::new(PgReputationRepo::new(pool.clone()));
        let endorsement_service = Arc::new(DefaultEndorsementService::new(
            reputation_repo.clone() as Arc<dyn ReputationRepo>
        )) as Arc<dyn EndorsementService>;
        self.reputation_repo = Some(reputation_repo as Arc<dyn ReputationRepo>);
        self.endorsement_service = Some(endorsement_service.clone());

        // Trust repo + graph reader (needed for constraint evaluation)
        let trust_repo = Arc::new(PgTrustRepo::new(pool.clone())) as Arc<dyn TrustRepo>;
        let trust_graph_reader = Arc::new(TrustRepoGraphReader::new(trust_repo.clone()))
            as Arc<dyn tc_engine_api::trust::TrustGraphReader>;

        // Rooms wiring (room CRUD only)
        self.include_rooms = true;
        let rooms_repo = Arc::new(PgRoomsRepo::new(pool.clone()));
        self.rooms_service = Some(Arc::new(DefaultRoomsService::new(
            rooms_repo as Arc<dyn RoomsRepo>,
        )) as Arc<dyn RoomsService>);

        // Polling wiring (polls, votes, dimensions, lifecycle, results)
        self.polling_service = Some(Arc::new(DefaultPollingService::new(
            pool.clone(),
            trust_graph_reader.clone(),
        )) as Arc<dyn PollingService>);

        // Engine registry + context (needed for create_room validate_config / on_room_created)
        let engine_ctx = EngineContext {
            pool: pool.clone(),
            trust_reader: trust_graph_reader,
            constraints: Arc::new(ConstraintRegistry),
            room_lifecycle: Arc::new(StubRoomLifecycle),
        };
        let mut engine_registry = EngineRegistry::new();
        engine_registry.register(PollingEngine::new());
        self.engine_registry = Some(Arc::new(engine_registry));
        self.engine_ctx = Some(engine_ctx);

        self.pool = Some(pool);

        // Suggestion endpoints require a ContentFilter extension; use NoopFilter in tests
        self.content_filter = Some(Arc::new(NoopFilter) as Arc<dyn ContentFilter>);

        self
    }

    /// Include trust routes with a real database pool.
    ///
    /// Wires identity (for auth), trust repo, and trust service. Used for
    /// integration tests against the trust HTTP endpoints.
    #[must_use]
    pub fn with_trust_pool(mut self, pool: PgPool) -> Self {
        // Identity wiring (needed for auth)
        self.include_identity = true;
        let identity_repo = Arc::new(PgIdentityRepo::new(pool.clone()));
        self.identity_repo = Some(Arc::clone(&identity_repo) as Arc<dyn IdentityRepo>);
        self.identity_service =
            Some(Arc::new(DefaultIdentityService::new(identity_repo)) as Arc<dyn IdentityService>);

        // Trust wiring
        self.include_trust = true;
        let trust_repo = Arc::new(PgTrustRepo::new(pool.clone())) as Arc<dyn TrustRepo>;
        let reputation_repo =
            Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
        let trust_service = Arc::new(DefaultTrustService::new(
            trust_repo.clone(),
            reputation_repo.clone(),
        )) as Arc<dyn TrustService>;
        self.trust_repo = Some(trust_repo);
        self.trust_service = Some(trust_service);
        self.reputation_repo = Some(reputation_repo);

        self.pool = Some(pool);
        self
    }

    /// Inject a pre-built stub [`TrustRepo`] without wiring up a database pool.
    ///
    /// Enables the trust routes and registers the supplied repo as the
    /// `Extension<Arc<dyn TrustRepo>>` extension.  Pair with
    /// [`with_stub_trust_service`] to provide the service extension as well.
    /// Use [`with_trust_pool`] instead when you need a real database.
    #[must_use]
    pub fn with_stub_trust_repo(mut self, repo: Arc<dyn TrustRepo>) -> Self {
        self.include_trust = true;
        self.trust_repo = Some(repo);
        self
    }

    /// Inject a pre-built stub [`TrustService`] without wiring up a database pool.
    ///
    /// Registers the supplied service as the `Extension<Arc<dyn TrustService>>`
    /// extension.  Pair with [`with_stub_trust_repo`] to provide the repo
    /// extension as well.
    #[must_use]
    pub fn with_stub_trust_service(mut self, service: Arc<dyn TrustService>) -> Self {
        self.trust_service = Some(service);
        self
    }

    /// Add a database pool as an Extension (for health check testing).
    ///
    /// Unlike [`with_identity_pool()`], this does NOT enable identity routes.
    /// Use this when you only need the pool available for the health check.
    #[must_use]
    pub fn with_pool(mut self, pool: PgPool) -> Self {
        self.pool = Some(pool);
        self
    }

    /// Include health check route (/health).
    #[must_use]
    pub fn with_health(mut self) -> Self {
        self.include_health = true;
        self
    }

    /// Include Swagger UI (/swagger-ui).
    #[must_use]
    pub fn with_swagger(mut self) -> Self {
        self.include_swagger = true;
        self
    }

    /// Configure CORS with specific allowed origins.
    ///
    /// Pass an empty slice to block all cross-origin requests.
    /// Pass `&["*"]` to allow any origin.
    #[must_use]
    pub fn with_cors(mut self, origins: &[&str]) -> Self {
        self.cors_origins = Some(origins.iter().map(|s| (*s).to_string()).collect());
        self
    }

    /// Disable CORS layer entirely.
    #[must_use]
    pub fn without_cors(mut self) -> Self {
        self.cors_origins = None;
        self
    }

    /// Enable security headers with default configuration.
    #[must_use]
    pub fn with_security_headers_default(mut self) -> Self {
        self.security_headers = Some(SecurityHeadersConfig::default());
        self
    }

    /// Enable security headers with custom configuration.
    #[must_use]
    pub fn with_security_headers(mut self, config: SecurityHeadersConfig) -> Self {
        self.security_headers = Some(config);
        self
    }

    /// Use a custom build info provider.
    #[must_use]
    pub fn with_build_info(mut self, provider: BuildInfo) -> Self {
        self.build_info = Some(provider);
        self
    }

    // =========================================================================
    // Build
    // =========================================================================

    /// Build the Axum router.
    ///
    /// The layer ordering matches main.rs exactly:
    /// 1. Routes (GraphQL, REST, Identity, Health, Swagger)
    /// 2. Extensions (schema, pool, repo, build_info)
    /// 3. CORS layer
    /// 4. Security headers middleware (outermost)
    #[must_use]
    pub fn build(self) -> Router {
        let build_info = self.build_info.unwrap_or_else(BuildInfo::from_env);

        // Build GraphQL schema
        let schema = Schema::build(QueryRoot, MutationRoot, EmptySubscription)
            .data(build_info.clone())
            .finish();

        // Start building the router
        let mut app = Router::new();

        // Add routes
        if self.include_graphql {
            app = app.route("/graphql", get(graphql_playground).post(graphql_handler));
        }

        if self.include_rest {
            let rest_v1 = Router::new().route("/build-info", get(rest::get_build_info));
            app = app.nest("/api/v1", rest_v1);
        }

        if self.include_swagger {
            app = app.merge(
                SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()),
            );
        }

        if self.include_identity {
            // Rate limiting disabled in tests — explicit opt-out per secure-defaults policy.
            let rl = tinycongress_api::config::RateLimitConfig {
                enabled: false,
                ..Default::default()
            };
            app = app.merge(identity::http::router(&rl));
        }

        if self.include_reputation {
            let rl = tinycongress_api::config::RateLimitConfig {
                enabled: false,
                ..Default::default()
            };
            app = app.merge(reputation::http::router(&rl));
        }

        if self.include_rooms {
            app = app.merge(rooms::http::router());
        }

        if self.include_trust {
            app = app.merge(trust::http::trust_router());
        }

        if self.include_health {
            app = app
                .route("/health", get(health_check))
                .route("/ready", get(readiness_check));
        }

        // Add extensions
        app = app.layer(Extension(schema)).layer(Extension(build_info));

        if let Some(pool) = self.pool {
            app = app.layer(Extension(pool));
        }

        if let Some(service) = self.identity_service {
            app = app.layer(Extension(service));
        }

        if let Some(repo) = self.identity_repo {
            app = app.layer(Extension(repo));
        }

        if let Some(service) = self.endorsement_service {
            app = app.layer(Extension(service));
        }

        if let Some(repo) = self.reputation_repo {
            app = app.layer(Extension(repo));
        }

        if let Some(service) = self.rooms_service {
            app = app.layer(Extension(service));
        }

        if let Some(service) = self.polling_service {
            app = app.layer(Extension(service));
        }

        if let Some(service) = self.trust_service {
            app = app.layer(Extension(service));
        }

        if let Some(repo) = self.trust_repo {
            app = app.layer(Extension(repo));
        }

        if let Some(registry) = self.engine_registry {
            app = app.layer(Extension(registry));
        }

        if let Some(ctx) = self.engine_ctx {
            app = app.layer(Extension(ctx));
        }

        if let Some(filter) = self.content_filter {
            app = app.layer(Extension(filter));
        }

        // Always provide a synthetic backup HMAC key when identity routes are active
        if self.include_identity {
            app = app.layer(Extension(SyntheticBackupKey::new(
                b"test-hmac-key-for-integration-tests".to_vec(),
            )));
        }

        // Add CORS layer if configured
        if let Some(origins) = self.cors_origins {
            let allow_origin: AllowOrigin = if origins.iter().any(|o| o == "*") {
                AllowOrigin::any()
            } else if origins.is_empty() {
                AllowOrigin::list(Vec::<HeaderValue>::new())
            } else {
                let header_values: Vec<HeaderValue> = origins
                    .iter()
                    .filter_map(|origin| origin.parse().ok())
                    .collect();
                AllowOrigin::list(header_values)
            };

            app = app.layer(
                CorsLayer::new()
                    .allow_methods([
                        Method::GET,
                        Method::POST,
                        Method::PUT,
                        Method::DELETE,
                        Method::PATCH,
                        Method::OPTIONS,
                    ])
                    .allow_headers(Any)
                    .allow_origin(allow_origin),
            );
        }

        // Add security headers middleware if configured
        if let Some(config) = self.security_headers {
            if config.enabled {
                let headers = build_security_headers(&config);
                app = app
                    .layer(middleware::from_fn(security_headers_middleware))
                    .layer(Extension(headers));
            }
        }

        app
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{
            header::{X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS},
            Request,
        },
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_minimal_builder_creates_health_route() {
        let app = TestAppBuilder::minimal().build();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_graphql_only_builder() {
        let app = TestAppBuilder::graphql_only().build();

        // GraphQL playground should be available
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/graphql")
                    .method("GET")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_with_mocks_builder() {
        let app = TestAppBuilder::with_mocks().build();

        // Health check should work
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);

        // GraphQL should work
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/graphql")
                    .method("GET")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_security_headers_applied() {
        let app = TestAppBuilder::minimal()
            .with_security_headers_default()
            .build();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(X_CONTENT_TYPE_OPTIONS),
            Some(&HeaderValue::from_static("nosniff"))
        );
        assert_eq!(
            response.headers().get(X_FRAME_OPTIONS),
            Some(&HeaderValue::from_static("DENY"))
        );
    }
}

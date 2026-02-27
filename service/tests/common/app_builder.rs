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

use async_graphql::{EmptySubscription, Schema};
use axum::{
    http::{header::HeaderValue, Method, StatusCode},
    middleware,
    response::IntoResponse,
    routing::get,
    Extension, Router,
};
use sqlx::{postgres::PgPoolOptions, PgPool};
use tinycongress_api::{
    build_info::BuildInfoProvider,
    config::SecurityHeadersConfig,
    graphql::{graphql_handler, graphql_playground, MutationRoot, QueryRoot},
    http::{build_security_headers, security_headers_middleware},
    identity,
    rest::{self, ApiDoc},
};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

/// Health check handler (mirrors main.rs)
async fn health_check() -> impl IntoResponse {
    StatusCode::OK
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
    /// Whether to include health check route
    include_health: bool,
    /// Whether to include Swagger UI
    include_swagger: bool,
    /// Custom build info provider (None uses from_env())
    build_info: Option<BuildInfoProvider>,
    /// Database pool for identity routes (lazy pool for validation-only tests)
    pool: Option<PgPool>,
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
            include_health: false,
            include_swagger: false,
            build_info: None,
            pool: None,
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

    /// Create a full app with a lazy pool (no real DB).
    ///
    /// Mirrors production main.rs wiring but with a lazy pool instead
    /// of a real database connection. Includes all routes, CORS, and
    /// security headers. Identity routes will only pass validation-only
    /// tests; DB-dependent tests belong in identity_handler_tests.rs.
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

    /// Include identity routes with a lazy pool (for validation-only tests).
    ///
    /// The lazy pool never actually connects to a database, so tests that
    /// exercise only the validation path (before any DB call) work fine.
    /// Tests that need real DB behaviour belong in identity_handler_tests.rs.
    #[must_use]
    pub fn with_identity_lazy(mut self) -> Self {
        self.include_identity = true;
        self.pool = Some(
            PgPoolOptions::new()
                .max_connections(1)
                .connect_lazy("postgres://fake:fake@localhost/fake")
                .expect("lazy pool"),
        );
        self
    }

    /// Include identity routes with a real database pool.
    #[must_use]
    pub fn with_identity_pool(mut self, pool: PgPool) -> Self {
        self.include_identity = true;
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
    pub fn with_build_info(mut self, provider: BuildInfoProvider) -> Self {
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
        let build_info = self.build_info.unwrap_or_else(BuildInfoProvider::from_env);

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
            app = app.merge(identity::http::router());
        }

        if self.include_health {
            app = app.route("/health", get(health_check));
        }

        // Add extensions
        app = app.layer(Extension(schema)).layer(Extension(build_info));

        if let Some(pool) = self.pool {
            app = app.layer(Extension(pool));
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

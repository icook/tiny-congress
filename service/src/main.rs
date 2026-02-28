#![deny(
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    clippy::todo,
    clippy::unimplemented,
    clippy::unwrap_used
)]

use async_graphql::{EmptySubscription, Schema};
use axum::{
    http::{header::HeaderValue, Method, StatusCode},
    middleware,
    response::IntoResponse,
    routing::get,
    Extension, Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tinycongress_api::{
    build_info::BuildInfo,
    config::Config,
    db::setup_database,
    graphql::{graphql_handler, graphql_playground, MutationRoot, QueryRoot},
    http::{build_security_headers, security_headers_middleware},
    identity::{
        self,
        repo::PgIdentityRepo,
        service::{DefaultIdentityService, IdentityService},
    },
    rest::{self, ApiDoc},
};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

// Health check handler
async fn health_check() -> impl IntoResponse {
    StatusCode::OK
}

/// Spawn a background task that cleans up expired request nonces.
fn spawn_nonce_cleanup(repo: Arc<dyn crate::identity::repo::IdentityRepo>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            match repo
                .cleanup_expired_nonces(crate::identity::http::auth::MAX_TIMESTAMP_SKEW)
                .await
            {
                Ok(0) => {}
                Ok(n) => tracing::debug!(count = n, "cleaned up expired nonces"),
                Err(e) => tracing::warn!("nonce cleanup failed: {e}"),
            }
        }
    });
}

fn build_cors_origin(origins: &[String]) -> AllowOrigin {
    if origins.iter().any(|o| o == "*") {
        tracing::warn!("CORS configured to allow any origin - not recommended for production");
        AllowOrigin::any()
    } else if origins.is_empty() {
        tracing::info!(
            "CORS allowed origins not configured - cross-origin requests will be blocked"
        );
        AllowOrigin::list(Vec::<HeaderValue>::new())
    } else {
        let mut header_values: Vec<HeaderValue> = Vec::with_capacity(origins.len());
        for origin in origins {
            match origin.parse() {
                Ok(v) => header_values.push(v),
                Err(e) => {
                    tracing::warn!(origin = %origin, error = %e, "Invalid CORS origin in config — skipping");
                }
            }
        }
        tracing::info!(origins = ?origins, "CORS allowed origins configured");
        AllowOrigin::list(header_values)
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Load and validate configuration first (fail-fast)
    let config = Config::load().map_err(|e| anyhow::anyhow!("{e}"))?;

    // Set up logging from config
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_new(&config.logging.level).map_err(|e| {
                anyhow::anyhow!("invalid log level '{}': {e}", config.logging.level)
            })?,
        )
        .init();

    // Init banner so container logs clearly show startup
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "tinycongress-api starting up"
    );

    // Database connection
    tracing::info!("Connecting to database...");
    let pool = setup_database(&config.database).await?;

    let build_info = BuildInfo::from_env();
    tracing::info!(
        version = %build_info.version,
        git_sha = %build_info.git_sha,
        build_time = %build_info.build_time,
        "resolved build metadata"
    );

    // Create the GraphQL schema
    let schema = Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(pool.clone()) // Pass the database pool to the schema
        .data(build_info.clone())
        .finish();

    let allow_origin = build_cors_origin(&config.cors.allowed_origins);

    // Build security headers layer if enabled
    let security_headers = if config.security_headers.enabled {
        tracing::info!("Security headers enabled");
        Some(build_security_headers(&config.security_headers))
    } else {
        tracing::info!("Security headers disabled");
        None
    };

    // REST API v1 routes
    let rest_v1 = Router::new().route("/build-info", get(rest::get_build_info));

    // Build the API
    let mut app = Router::new()
        // GraphQL endpoint - POST always enabled, GET (playground) is conditional
        .route("/graphql", {
            let route = axum::routing::post(graphql_handler);
            if config.graphql.playground_enabled {
                tracing::info!("GraphQL Playground enabled at /graphql");
                route.get(graphql_playground)
            } else {
                tracing::info!(
                    "GraphQL Playground disabled (enable via TC_GRAPHQL__PLAYGROUND_ENABLED=true)"
                );
                route
            }
        })
        // REST API v1
        .nest("/api/v1", rest_v1)
        // Identity routes
        .merge(identity::http::router())
        // Health check route
        .route("/health", get(health_check))
        // Add the schema to the extension
        .layer(Extension(schema));

    // Wire identity layers: repo for AuthenticatedDevice, service for signup
    let identity_repo: Arc<dyn crate::identity::repo::IdentityRepo> =
        Arc::new(PgIdentityRepo::new(pool.clone()));
    let identity_service: Arc<dyn IdentityService> =
        Arc::new(DefaultIdentityService::new(identity_repo.clone()));
    // Background task: clean up expired request nonces every 60 seconds
    spawn_nonce_cleanup(identity_repo.clone());

    app = app
        .layer(Extension(identity_repo))
        .layer(Extension(identity_service))
        .layer(Extension(build_info))
        .layer(
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

    // Add security headers middleware if enabled
    if let Some(headers) = security_headers {
        app = app
            .layer(middleware::from_fn(security_headers_middleware))
            .layer(Extension(headers));
    }

    // Add Swagger UI if enabled (disabled by default for security)
    if config.swagger.enabled {
        tracing::info!("Swagger UI enabled at /swagger-ui");
        app = app
            .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()));
    } else {
        tracing::info!("Swagger UI disabled (enable via TC_SWAGGER__ENABLED=true)");
    }

    // Start the server
    let addr = SocketAddr::from(([0, 0, 0, 0], config.server.port));
    tracing::info!(
        graphql = %format!("http://{}/graphql", addr),
        rest = %format!("http://{}/api/v1", addr),
        "Starting server"
    );

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

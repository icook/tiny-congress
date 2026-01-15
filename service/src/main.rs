//! `TinyCongress` API server binary.
//!
//! Starts the HTTP server with GraphQL and REST endpoints.

#![deny(
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    clippy::todo,
    clippy::unimplemented,
    clippy::unwrap_used
)]

use std::sync::Arc;

use async_graphql::{EmptySubscription, Schema};
use axum::{
    http::{header::HeaderValue, Method, StatusCode},
    middleware,
    response::IntoResponse,
    routing::get,
    Extension, Router,
};
use std::net::SocketAddr;
use tinycongress_api::{
    build_info::BuildInfoProvider,
    config::Config,
    db::setup_database,
    graphql::{graphql_handler, graphql_playground, MutationRoot, QueryRoot},
    http::{build_security_headers, security_headers_middleware},
    identity::{self, repo::PgAccountRepo},
    rest::{self, ApiDoc},
};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

// Health check handler
async fn health_check() -> impl IntoResponse {
    StatusCode::OK
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Load and validate configuration first (fail-fast)
    let config = Config::load().map_err(|e| anyhow::anyhow!("{e}"))?;

    // Set up logging from config
    std::env::set_var("RUST_LOG", &config.logging.level);
    tracing_subscriber::fmt::init();

    // Init banner so container logs clearly show startup
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "tinycongress-api starting up"
    );

    // Database connection
    tracing::info!("Connecting to database...");
    let pool = setup_database(&config.database).await?;

    let build_info = BuildInfoProvider::from_env();
    let build_info_snapshot = build_info.build_info();
    tracing::info!(
        version = %build_info_snapshot.version,
        git_sha = %build_info_snapshot.git_sha,
        build_time = %build_info_snapshot.build_time,
        "resolved build metadata"
    );

    // Create the GraphQL schema
    let schema = Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(pool.clone()) // Pass the database pool to the schema
        .data(build_info.clone())
        .finish();

    // Create repositories
    let account_repo: Arc<dyn identity::repo::AccountRepo> =
        Arc::new(PgAccountRepo::new(pool.clone()));

    // Build CORS layer from config
    let cors_origins = &config.cors.allowed_origins;
    let allow_origin: AllowOrigin = if cors_origins.iter().any(|o| o == "*") {
        tracing::warn!("CORS configured to allow any origin - not recommended for production");
        AllowOrigin::any()
    } else if cors_origins.is_empty() {
        tracing::info!(
            "CORS allowed origins not configured - cross-origin requests will be blocked"
        );
        AllowOrigin::list(Vec::<HeaderValue>::new())
    } else {
        let origins: Vec<HeaderValue> = cors_origins
            .iter()
            .filter_map(|origin| origin.parse().ok())
            .collect();
        tracing::info!(origins = ?cors_origins, "CORS allowed origins configured");
        AllowOrigin::list(origins)
    };

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
        .layer(Extension(schema))
        .layer(Extension(pool.clone()))
        .layer(Extension(account_repo))
        .layer(Extension(build_info))
        .layer(
            CorsLayer::new()
                .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
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

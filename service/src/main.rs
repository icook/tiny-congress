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
};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

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
        .data(build_info)
        .finish();

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

    // Build the API
    let app = Router::new()
        // GraphQL routes
        .route("/graphql", get(graphql_playground).post(graphql_handler))
        // Health check route
        .route("/health", get(health_check))
        // Add the schema to the extension
        .layer(Extension(schema))
        .layer(Extension(pool))
        .layer(
            CorsLayer::new()
                .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
                .allow_headers(Any)
                .allow_origin(allow_origin),
        );

    // Start the server
    let addr = SocketAddr::from(([0, 0, 0, 0], config.server.port));
    tracing::info!("Starting server at http://{}/graphql", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

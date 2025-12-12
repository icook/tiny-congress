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
    http::Method,
    routing::get,
    Extension, Router,
};
use std::net::SocketAddr;
use tinycongress_api::{
    build_info::BuildInfoProvider,
    db::setup_database,
    graphql::{graphql_handler, graphql_playground, MutationRoot, QueryRoot},
    identity, observability,
};
use tower_http::cors::{Any, CorsLayer};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Set up logging
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    tracing_subscriber::fmt::init();

    // Init banner so container logs clearly show startup
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "tinycongress-api starting up"
    );

    // Database connection
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/tinycongress".to_string());

    // Initialize metrics
    tracing::info!("Initializing Prometheus metrics...");
    let metrics_handle = observability::init_metrics();

    tracing::info!("Connecting to database...");
    let pool = setup_database(&database_url).await?;

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

    // Build the API
    let app = Router::new()
        // GraphQL routes
        .route("/graphql", get(graphql_playground).post(graphql_handler))
        .merge(identity::http::router())
        // Observability routes
        .route("/health", get(observability::health_check))
        .route("/metrics", get(observability::metrics_handler))
        // Add shared state to extensions
        .layer(Extension(schema))
        .layer(Extension(pool))
        .layer(Extension(metrics_handle))
        .layer(
            CorsLayer::new()
                .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
                .allow_headers(Any)
                .allow_origin(Any),
        );

    // Start the server
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()
        .unwrap_or(8080);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Starting server at http://{}/graphql", addr);

    // Updated server binding code
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

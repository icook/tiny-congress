use axum::{
    routing::get,
    Router,
    Extension,
    http::StatusCode,
    response::IntoResponse,
};
use std::net::SocketAddr;
use async_graphql::{EmptySubscription, Schema};
use prioritization_room::{
    graphql::{QueryRoot, MutationRoot, graphql_playground, graphql_handler},
    db::{setup_database, create_seed_data},
};
use sqlx::PgPool;

// Health check handler
async fn health_check() -> impl IntoResponse {
    StatusCode::OK
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Set up logging
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    tracing_subscriber::fmt::init();
    
    // Database connection
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/prioritization".to_string());
    
    tracing::info!("Connecting to database...");
    let pool = setup_database(&database_url).await?;
    
    // Create seed data
    tracing::info!("Setting up seed data...");
    create_seed_data(&pool).await?;
    
    // Create the GraphQL schema
    let schema = Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(pool.clone()) // Pass the database pool to the schema
        .finish();
    
    // Build the API
    let app = Router::new()
        // GraphQL routes
        .route("/graphql", get(graphql_playground).post(graphql_handler))
        // Health check route
        .route("/health", get(health_check))
        // Add the schema to the extension
        .layer(Extension(schema))
        .layer(Extension(pool));
    
    // Start the server
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()
        .unwrap_or(8080);
    
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Starting server at http://{}/graphql", addr);
    
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;
    
    Ok(())
}
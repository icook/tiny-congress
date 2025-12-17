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
    extract::Request,
    http::{
        header::{
            HeaderName, HeaderValue, CONTENT_SECURITY_POLICY, REFERRER_POLICY,
            STRICT_TRANSPORT_SECURITY, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS, X_XSS_PROTECTION,
        },
        Method, StatusCode,
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Extension, Router,
};
use std::{net::SocketAddr, sync::Arc};
use tinycongress_api::{
    build_info::BuildInfoProvider,
    config::{Config, SecurityHeadersConfig},
    db::setup_database,
    graphql::{graphql_handler, graphql_playground, MutationRoot, QueryRoot},
};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

// Health check handler
async fn health_check() -> impl IntoResponse {
    StatusCode::OK
}

/// Build security headers from configuration.
fn build_security_headers(config: &SecurityHeadersConfig) -> Arc<Vec<(HeaderName, HeaderValue)>> {
    let mut headers = Vec::new();

    // X-Content-Type-Options: nosniff (always)
    headers.push((X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")));

    // X-Frame-Options
    if let Ok(value) = HeaderValue::from_str(&config.frame_options) {
        headers.push((X_FRAME_OPTIONS, value));
    }

    // X-XSS-Protection (legacy but still useful for older browsers)
    headers.push((X_XSS_PROTECTION, HeaderValue::from_static("1; mode=block")));

    // Content-Security-Policy
    if let Ok(value) = HeaderValue::from_str(&config.content_security_policy) {
        headers.push((CONTENT_SECURITY_POLICY, value));
    }

    // Referrer-Policy
    if let Ok(value) = HeaderValue::from_str(&config.referrer_policy) {
        headers.push((REFERRER_POLICY, value));
    }

    // HSTS (only if enabled - should only be used with HTTPS)
    if config.hsts_enabled {
        let hsts_value = if config.hsts_include_subdomains {
            format!("max-age={}; includeSubDomains", config.hsts_max_age)
        } else {
            format!("max-age={}", config.hsts_max_age)
        };
        if let Ok(value) = HeaderValue::from_str(&hsts_value) {
            headers.push((STRICT_TRANSPORT_SECURITY, value));
        }
    }

    Arc::new(headers)
}

/// Middleware to add security headers to all responses.
async fn security_headers_middleware(
    Extension(headers): Extension<Arc<Vec<(HeaderName, HeaderValue)>>>,
    request: Request,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;
    let response_headers = response.headers_mut();
    for (name, value) in headers.iter() {
        response_headers.insert(name.clone(), value.clone());
    }
    response
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

    // Build security headers layer if enabled
    let security_headers = if config.security_headers.enabled {
        tracing::info!("Security headers enabled");
        Some(build_security_headers(&config.security_headers))
    } else {
        tracing::info!("Security headers disabled");
        None
    };

    // Build the API
    let mut app = Router::new()
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

    // Add security headers middleware if enabled
    if let Some(headers) = security_headers {
        app = app
            .layer(middleware::from_fn(security_headers_middleware))
            .layer(Extension(headers));
    }

    // Start the server
    let addr = SocketAddr::from(([0, 0, 0, 0], config.server.port));
    tracing::info!("Starting server at http://{}/graphql", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

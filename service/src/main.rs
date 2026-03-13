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
use axum_prometheus::PrometheusMetricLayer;
use sqlx::PgPool;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tinycongress_api::{
    build_info::BuildInfo,
    config::Config,
    db::setup_database,
    graphql::{graphql_handler, graphql_playground, MutationRoot, QueryRoot},
    http::{build_security_headers, security_headers_middleware},
    identity::{
        self,
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
        repo::{PgRoomsRepo, RoomsRepo},
        service::{DefaultRoomsService, RoomsService},
    },
    trust::{
        self,
        engine::TrustEngine,
        repo::{PgTrustRepo, TrustRepo},
        service::{DefaultTrustService, TrustService},
        worker::TrustWorker,
    },
};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

/// Liveness check — confirms the process is alive.
///
/// Always returns 200. Used by Kubernetes startup and liveness probes.
// perf-test: trigger BuildKit remote driver benchmark
async fn health_check() -> impl IntoResponse {
    StatusCode::OK
}

/// Readiness check — verifies the API can reach postgres.
///
/// Returns 200 if a pooled connection can be acquired within 2 seconds,
/// 503 otherwise. Used by the Kubernetes readiness probe so traffic is
/// only routed to pods with a healthy database connection.
async fn readiness_check(pool: Option<Extension<PgPool>>) -> impl IntoResponse {
    let Some(Extension(pool)) = pool else {
        return StatusCode::SERVICE_UNAVAILABLE;
    };
    match tokio::time::timeout(Duration::from_secs(2), pool.acquire()).await {
        Ok(Ok(_)) => StatusCode::OK,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    }
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

/// Spawn a background task that periodically deletes expired nonces.
///
/// TTL matches [`identity::http::auth::MAX_TIMESTAMP_SKEW`] so nonces
/// outlive the timestamp validation window.
fn spawn_nonce_cleanup(pool: sqlx::PgPool) {
    tokio::spawn(async move {
        let ttl = identity::http::auth::MAX_TIMESTAMP_SKEW;
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            match identity::repo::cleanup_expired_nonces(&pool, ttl).await {
                Ok(0) => {}
                Ok(n) => tracing::debug!(count = n, "Cleaned up expired nonces"),
                Err(e) => tracing::warn!("Nonce cleanup failed: {e}"),
            }
        }
    });
}

/// Build the Axum router with all service layers wired up.
#[allow(clippy::too_many_lines)]
async fn build_app(
    config: &Config,
    pool: PgPool,
    build_info: BuildInfo,
    schema: Schema<QueryRoot, MutationRoot, EmptySubscription>,
    allow_origin: AllowOrigin,
) -> Result<(Router, PgPool), anyhow::Error> {
    let rest_v1 = Router::new().route("/build-info", get(rest::get_build_info));

    // Identity wiring
    let repo = Arc::new(PgIdentityRepo::new(pool.clone()));
    let service = Arc::new(DefaultIdentityService::new(repo.clone())) as Arc<dyn IdentityService>;
    let repo_ext = repo as Arc<dyn IdentityRepo>;

    let synthetic_backup_key = identity::http::backup::SyntheticBackupKey::new(
        config.synthetic_backup_key.as_bytes().to_vec(),
    );

    // Reputation wiring
    let reputation_repo = Arc::new(PgReputationRepo::new(pool.clone()));
    let endorsement_service = Arc::new(DefaultEndorsementService::new(reputation_repo.clone()))
        as Arc<dyn EndorsementService>;
    let reputation_repo_for_worker = reputation_repo.clone() as Arc<dyn ReputationRepo>;
    let reputation_repo_ext = reputation_repo as Arc<dyn ReputationRepo>;

    // Bootstrap configured verifier accounts
    let bootstrapped_verifiers =
        reputation::bootstrap::bootstrap_verifiers(&pool, &config.verifiers)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to bootstrap verifiers: {e}"))?;

    // Find the ID.me verifier account ID if ID.me is configured
    let idme_verifier_account_id = if config.idme.is_some() {
        bootstrapped_verifiers
            .iter()
            .find(|v| v.name == "idme")
            .map(|v| v.account_id)
    } else {
        None
    };

    // Trust repo (needed for room constraint evaluation, trust worker, and HTTP endpoints)
    let trust_repo = Arc::new(PgTrustRepo::new(pool.clone())) as Arc<dyn TrustRepo>;
    let trust_repo_for_worker = trust_repo.clone();
    let trust_repo_for_service = trust_repo.clone();
    let trust_repo_for_http = trust_repo.clone();

    // Trust engine and service
    let trust_engine = Arc::new(TrustEngine::new(pool.clone()));
    let reputation_repo_for_trust =
        Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let trust_service: Arc<dyn TrustService> = Arc::new(DefaultTrustService::new(
        trust_repo_for_service,
        reputation_repo_for_trust,
    ));

    // Rooms wiring
    let rooms_repo = Arc::new(PgRoomsRepo::new(pool.clone()));
    let rooms_service = Arc::new(DefaultRoomsService::new(
        rooms_repo as Arc<dyn RoomsRepo>,
        trust_repo,
    )) as Arc<dyn RoomsService>;

    let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();

    let app = Router::new()
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
        .nest("/api/v1", rest_v1)
        .merge(identity::http::router())
        .merge(reputation::http::router())
        .merge(rooms::http::router())
        .merge(trust::http::trust_router())
        .route("/health", get(health_check))
        .route("/ready", get(readiness_check))
        .route("/metrics", get(|| async move { metric_handle.render() }))
        .layer(Extension(schema))
        .layer(Extension(service))
        .layer(Extension(repo_ext))
        .layer(Extension(endorsement_service))
        .layer(Extension(reputation_repo_ext))
        .layer(Extension(rooms_service))
        .layer(Extension(trust_service))
        .layer(Extension(trust_repo_for_http))
        .layer(Extension(trust_engine.clone()))
        .layer(Extension(synthetic_backup_key))
        .layer(Extension(build_info))
        .layer(Extension(pool.clone()));

    // Add ID.me config extension if configured
    let app = if let Some(ref idme_config) = config.idme {
        tracing::info!("ID.me verification enabled");
        let mut app = app.layer(Extension(Arc::new(idme_config.clone())));
        if let Some(verifier_id) = idme_verifier_account_id {
            app = app.layer(Extension(reputation::http::idme::IdMeVerifierAccountId(
                verifier_id,
            )));
        }
        app
    } else {
        tracing::info!("ID.me verification disabled (no TC_IDME__* config)");
        app
    };

    let app = app.layer(
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

    let app = app.layer(prometheus_layer);

    // Spawn trust background worker
    let trust_worker = Arc::new(TrustWorker::new(
        trust_repo_for_worker,
        reputation_repo_for_worker,
        trust_engine,
        config.trust.batch_size,
        config.trust.batch_interval_secs,
    ));
    tokio::spawn(async move { trust_worker.run().await });

    Ok((app, pool))
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

    // Service wiring
    let (app, pool_for_cleanup) =
        build_app(&config, pool.clone(), build_info, schema, allow_origin).await?;
    let mut app = app;

    spawn_nonce_cleanup(pool_for_cleanup);

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

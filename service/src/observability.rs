use axum::{http::StatusCode, response::IntoResponse, Extension};
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use sqlx::PgPool;

/// Initialize Prometheus metrics exporter
///
/// # Panics
/// Panics if the Prometheus exporter cannot be installed
#[must_use]
#[allow(clippy::panic)]
#[allow(clippy::expect_used)]
pub fn init_metrics() -> PrometheusHandle {
    PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Full("reducer.replay_seconds".to_string()),
            &[0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0],
        )
        .unwrap_or_else(|e| panic!("Failed to set metric buckets: {e}"))
        .install_recorder()
        .expect("Failed to install Prometheus recorder")
}

/// GET /health endpoint
///
/// Returns 200 OK if the service is healthy and can connect to the database
pub async fn health_check(Extension(pool): Extension<PgPool>) -> impl IntoResponse {
    // Simple DB connectivity check
    match sqlx::query("SELECT 1").fetch_one(&pool).await {
        Ok(_) => (StatusCode::OK, "OK"),
        Err(e) => {
            tracing::error!(error = %e, "Health check failed: database connectivity error");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "Database connectivity error",
            )
        }
    }
}

/// GET /metrics endpoint
///
/// Returns Prometheus metrics in text format
#[must_use]
#[allow(clippy::unused_async)]
pub async fn metrics_handler(Extension(handle): Extension<PrometheusHandle>) -> impl IntoResponse {
    handle.render()
}

/// Record auth success metric
pub fn record_auth_success() {
    metrics::counter!("auth.success").increment(1);
}

/// Record auth failure metric
pub fn record_auth_failure() {
    metrics::counter!("auth.failure").increment(1);
}

/// Record revoked device attempt
pub fn record_revoked_device_attempt() {
    metrics::counter!("device.revoked_attempt").increment(1);
}

/// Record endorsement write
pub fn record_endorsement_write() {
    metrics::counter!("endorsement.write").increment(1);
}

/// Record endorsement revocation
pub fn record_endorsement_revocation() {
    metrics::counter!("endorsement.revocation").increment(1);
}

/// Record reducer replay time
pub fn record_reducer_replay_seconds(seconds: f64) {
    metrics::histogram!("reducer.replay_seconds").record(seconds);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_recording() {
        // Initialize in-memory recorder for testing
        let _ = init_metrics();

        // Record some metrics
        record_auth_success();
        record_auth_success();
        record_auth_failure();
        record_endorsement_write();
        record_reducer_replay_seconds(0.5);

        // We can't easily assert on the values without pulling from the handle,
        // but this tests that the metrics don't panic
    }
}

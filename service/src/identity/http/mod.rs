use axum::routing::post;
use axum::Router;

pub mod accounts;
pub mod devices;
pub mod sessions;

pub fn router() -> Router {
    Router::new()
        .route("/auth/signup", post(accounts::signup))
        .route("/auth/challenge", post(sessions::issue_challenge))
        .route("/auth/verify", post(sessions::verify_challenge))
        .route("/me/devices/add", post(devices::add_device))
        .route(
            "/me/devices/{device_id}/revoke",
            post(devices::revoke_device),
        )
}

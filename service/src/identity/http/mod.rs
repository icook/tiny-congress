use axum::routing::post;
use axum::Router;

pub mod accounts;
pub mod devices;

pub fn router() -> Router {
    Router::new()
        .route("/auth/signup", post(accounts::signup))
        .route("/me/devices/add", post(devices::add_device))
        .route(
            "/me/devices/{device_id}/revoke",
            post(devices::revoke_device),
        )
}

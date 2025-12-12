use axum::routing::{get, post};
use axum::Router;

pub mod accounts;
pub mod devices;
pub mod endorsements;
pub mod recovery;
pub mod sessions;

pub fn router() -> Router {
    Router::new()
        .route("/auth/signup", post(accounts::signup))
        .route("/auth/challenge", post(sessions::issue_challenge))
        .route("/auth/verify", post(sessions::verify_challenge))
        .route("/endorsements", post(endorsements::create_endorsement))
        .route(
            "/endorsements/{id}/revoke",
            post(endorsements::revoke_endorsement),
        )
        .route("/me/devices/add", post(devices::add_device))
        .route(
            "/me/devices/{device_id}/revoke",
            post(devices::revoke_device),
        )
        .route(
            "/me/recovery_policy",
            get(recovery::get_recovery_policy).post(recovery::set_recovery_policy),
        )
        .route("/recovery/approve", post(recovery::approve_recovery))
        .route("/recovery/rotate_root", post(recovery::rotate_root))
}

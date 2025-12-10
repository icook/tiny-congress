use axum::routing::post;
use axum::Router;

pub mod accounts;

pub fn router() -> Router {
    Router::new().route("/auth/signup", post(accounts::signup))
}

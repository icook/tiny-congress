pub mod bot;
pub mod engine;
// TODO: Move HTTP handlers from service/src/rooms/http/polling.rs to this crate
// once AuthenticatedDevice extractor is extracted from the service crate.
pub mod lifecycle;
pub mod repo;
pub mod service;

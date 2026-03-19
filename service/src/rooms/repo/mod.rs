//! Repository layer for rooms persistence

pub mod rooms;

// Poll/vote/evidence/lifecycle modules now live in tc-engine-polling.
// Re-export them here for backward compatibility.
pub use tc_engine_polling::repo::evidence;
pub use tc_engine_polling::repo::lifecycle_queue;
pub use tc_engine_polling::repo::polls;
pub use tc_engine_polling::repo::votes;

pub use lifecycle_queue::{
    enqueue_lifecycle_event, read_lifecycle_event, LifecycleMessage, LifecyclePayload,
};
pub use polls::{DimensionRecord, PollRecord, PollRepoError};
pub use rooms::{RoomRecord, RoomRepoError};
pub use votes::{BucketCount, DimensionDistribution, DimensionStats, VoteRecord, VoteRepoError};

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

/// Repository trait for room CRUD operations.
///
/// Polling-specific operations (polls, votes, dimensions, lifecycle) have been
/// removed from this trait — they are now accessed directly through
/// `tc_engine_polling::repo` by `DefaultPollingService`. No callers use poll/vote
/// methods via `dyn RoomsRepo`; confirmed by grep across `service/src/`.
#[async_trait]
pub trait RoomsRepo: Send + Sync {
    async fn create_room(
        &self,
        name: &str,
        description: Option<&str>,
        eligibility_topic: &str,
        poll_duration_secs: Option<i32>,
        constraint_type: &str,
        constraint_config: &serde_json::Value,
    ) -> Result<RoomRecord, RoomRepoError>;
    async fn list_rooms(&self, status: Option<&str>) -> Result<Vec<RoomRecord>, RoomRepoError>;
    async fn get_room(&self, room_id: Uuid) -> Result<RoomRecord, RoomRepoError>;
    async fn update_room_status(&self, room_id: Uuid, status: &str) -> Result<(), RoomRepoError>;
    async fn rooms_needing_content(&self) -> Result<Vec<RoomRecord>, RoomRepoError>;
}

/// `PostgreSQL` implementation of [`RoomsRepo`].
pub struct PgRoomsRepo {
    pool: PgPool,
}

impl PgRoomsRepo {
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RoomsRepo for PgRoomsRepo {
    async fn create_room(
        &self,
        name: &str,
        description: Option<&str>,
        eligibility_topic: &str,
        poll_duration_secs: Option<i32>,
        constraint_type: &str,
        constraint_config: &serde_json::Value,
    ) -> Result<RoomRecord, RoomRepoError> {
        rooms::create_room(
            &self.pool,
            name,
            description,
            eligibility_topic,
            poll_duration_secs,
            constraint_type,
            constraint_config,
        )
        .await
    }

    async fn list_rooms(&self, status: Option<&str>) -> Result<Vec<RoomRecord>, RoomRepoError> {
        rooms::list_rooms(&self.pool, status).await
    }

    async fn get_room(&self, room_id: Uuid) -> Result<RoomRecord, RoomRepoError> {
        rooms::get_room(&self.pool, room_id).await
    }

    async fn update_room_status(&self, room_id: Uuid, status: &str) -> Result<(), RoomRepoError> {
        rooms::update_room_status(&self.pool, room_id, status).await
    }

    async fn rooms_needing_content(&self) -> Result<Vec<RoomRecord>, RoomRepoError> {
        rooms::rooms_needing_content(&self.pool).await
    }
}

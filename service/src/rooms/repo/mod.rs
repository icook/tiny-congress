//! Repository layer for rooms persistence

pub mod polls;
pub mod rooms;
pub mod votes;

pub use polls::{DimensionRecord, PollRecord, PollRepoError};
pub use rooms::{RoomRecord, RoomRepoError};
pub use votes::{DimensionStats, VoteRecord, VoteRepoError};

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

/// Consolidated repository trait for rooms persistence.
#[async_trait]
pub trait RoomsRepo: Send + Sync {
    // Room operations
    async fn create_room(
        &self,
        name: &str,
        description: Option<&str>,
        eligibility_topic: &str,
    ) -> Result<RoomRecord, RoomRepoError>;
    async fn list_rooms(&self, status: Option<&str>) -> Result<Vec<RoomRecord>, RoomRepoError>;
    async fn get_room(&self, room_id: Uuid) -> Result<RoomRecord, RoomRepoError>;
    async fn update_room_status(&self, room_id: Uuid, status: &str) -> Result<(), RoomRepoError>;

    // Poll operations
    async fn create_poll(
        &self,
        room_id: Uuid,
        question: &str,
        description: Option<&str>,
    ) -> Result<PollRecord, PollRepoError>;
    async fn list_polls_by_room(&self, room_id: Uuid) -> Result<Vec<PollRecord>, PollRepoError>;
    async fn get_poll(&self, poll_id: Uuid) -> Result<PollRecord, PollRepoError>;
    async fn update_poll_status(&self, poll_id: Uuid, status: &str) -> Result<(), PollRepoError>;

    // Dimension operations
    async fn create_dimension(
        &self,
        poll_id: Uuid,
        name: &str,
        description: Option<&str>,
        min_value: f32,
        max_value: f32,
        sort_order: i32,
    ) -> Result<DimensionRecord, PollRepoError>;
    async fn list_dimensions(&self, poll_id: Uuid) -> Result<Vec<DimensionRecord>, PollRepoError>;

    // Vote operations
    async fn upsert_vote(
        &self,
        poll_id: Uuid,
        dimension_id: Uuid,
        user_id: Uuid,
        value: f32,
    ) -> Result<VoteRecord, VoteRepoError>;
    async fn get_user_votes(
        &self,
        poll_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<VoteRecord>, VoteRepoError>;
    async fn count_voters(&self, poll_id: Uuid) -> Result<i64, VoteRepoError>;
    async fn compute_poll_stats(&self, poll_id: Uuid)
        -> Result<Vec<DimensionStats>, VoteRepoError>;
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
    ) -> Result<RoomRecord, RoomRepoError> {
        rooms::create_room(&self.pool, name, description, eligibility_topic).await
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

    async fn create_poll(
        &self,
        room_id: Uuid,
        question: &str,
        description: Option<&str>,
    ) -> Result<PollRecord, PollRepoError> {
        polls::create_poll(&self.pool, room_id, question, description).await
    }

    async fn list_polls_by_room(&self, room_id: Uuid) -> Result<Vec<PollRecord>, PollRepoError> {
        polls::list_polls_by_room(&self.pool, room_id).await
    }

    async fn get_poll(&self, poll_id: Uuid) -> Result<PollRecord, PollRepoError> {
        polls::get_poll(&self.pool, poll_id).await
    }

    async fn update_poll_status(&self, poll_id: Uuid, status: &str) -> Result<(), PollRepoError> {
        polls::update_poll_status(&self.pool, poll_id, status).await
    }

    async fn create_dimension(
        &self,
        poll_id: Uuid,
        name: &str,
        description: Option<&str>,
        min_value: f32,
        max_value: f32,
        sort_order: i32,
    ) -> Result<DimensionRecord, PollRepoError> {
        polls::create_dimension(
            &self.pool,
            poll_id,
            name,
            description,
            min_value,
            max_value,
            sort_order,
        )
        .await
    }

    async fn list_dimensions(&self, poll_id: Uuid) -> Result<Vec<DimensionRecord>, PollRepoError> {
        polls::list_dimensions(&self.pool, poll_id).await
    }

    async fn upsert_vote(
        &self,
        poll_id: Uuid,
        dimension_id: Uuid,
        user_id: Uuid,
        value: f32,
    ) -> Result<VoteRecord, VoteRepoError> {
        votes::upsert_vote(&self.pool, poll_id, dimension_id, user_id, value).await
    }

    async fn get_user_votes(
        &self,
        poll_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<VoteRecord>, VoteRepoError> {
        votes::get_user_votes(&self.pool, poll_id, user_id).await
    }

    async fn count_voters(&self, poll_id: Uuid) -> Result<i64, VoteRepoError> {
        votes::count_voters(&self.pool, poll_id).await
    }

    async fn compute_poll_stats(
        &self,
        poll_id: Uuid,
    ) -> Result<Vec<DimensionStats>, VoteRepoError> {
        votes::compute_poll_stats(&self.pool, poll_id).await
    }
}

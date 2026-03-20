//! Repository layer for polling engine persistence
//!
//! Contains poll, vote, evidence, and lifecycle queue operations.
//! These are pure SQL functions with no service-layer dependencies.

pub mod bot_traces;
pub mod evidence;
pub mod lifecycle_queue;
pub mod pgmq;
pub mod polls;
pub mod votes;

pub use evidence::{EvidenceRecord, NewEvidence};
pub use lifecycle_queue::{
    archive_lifecycle_event, enqueue_lifecycle_event, is_poison, read_lifecycle_event,
    LifecycleMessage, LifecyclePayload,
};
pub use polls::{DimensionRecord, PollRecord, PollRepoError};
pub use votes::{BucketCount, DimensionDistribution, DimensionStats, VoteRecord, VoteRepoError};

//! Repository layer for reputation persistence

pub mod endorsements;
pub mod external_identities;

pub use endorsements::{
    count_active_trust_endorsements_by, create_endorsement, has_endorsement,
    list_endorsements_by_subject, revoke_endorsement, CreatedEndorsement, EndorsementRecord,
    EndorsementRepoError,
};
pub use external_identities::{
    get_external_identity_by_provider, link_external_identity, ExternalIdentityRecord,
    ExternalIdentityRepoError,
};

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

/// Consolidated repository trait for reputation persistence.
#[async_trait]
pub trait ReputationRepo: Send + Sync {
    // Endorsement operations

    async fn create_endorsement(
        &self,
        subject_id: Uuid,
        topic: &str,
        endorser_id: Option<Uuid>,
        evidence: Option<&serde_json::Value>,
        weight: f32,
        attestation: Option<&serde_json::Value>,
    ) -> Result<CreatedEndorsement, EndorsementRepoError>;

    async fn has_endorsement(
        &self,
        subject_id: Uuid,
        topic: &str,
    ) -> Result<bool, EndorsementRepoError>;

    async fn list_endorsements_by_subject(
        &self,
        subject_id: Uuid,
    ) -> Result<Vec<EndorsementRecord>, EndorsementRepoError>;

    async fn revoke_endorsement(
        &self,
        endorser_id: Uuid,
        subject_id: Uuid,
        topic: &str,
    ) -> Result<(), EndorsementRepoError>;

    async fn count_active_trust_endorsements_by(
        &self,
        endorser_id: Uuid,
    ) -> Result<i64, EndorsementRepoError>;

    // External identity operations

    async fn link_external_identity(
        &self,
        account_id: Uuid,
        provider: &str,
        provider_subject: &str,
    ) -> Result<ExternalIdentityRecord, ExternalIdentityRepoError>;

    async fn get_external_identity_by_provider(
        &self,
        provider: &str,
        provider_subject: &str,
    ) -> Result<ExternalIdentityRecord, ExternalIdentityRepoError>;
}

/// `PostgreSQL` implementation of [`ReputationRepo`].
pub struct PgReputationRepo {
    pool: PgPool,
}

impl PgReputationRepo {
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ReputationRepo for PgReputationRepo {
    async fn create_endorsement(
        &self,
        subject_id: Uuid,
        topic: &str,
        endorser_id: Option<Uuid>,
        evidence: Option<&serde_json::Value>,
        weight: f32,
        attestation: Option<&serde_json::Value>,
    ) -> Result<CreatedEndorsement, EndorsementRepoError> {
        endorsements::create_endorsement(
            &self.pool,
            subject_id,
            topic,
            endorser_id,
            evidence,
            weight,
            attestation,
        )
        .await
    }

    async fn has_endorsement(
        &self,
        subject_id: Uuid,
        topic: &str,
    ) -> Result<bool, EndorsementRepoError> {
        endorsements::has_endorsement(&self.pool, subject_id, topic).await
    }

    async fn list_endorsements_by_subject(
        &self,
        subject_id: Uuid,
    ) -> Result<Vec<EndorsementRecord>, EndorsementRepoError> {
        endorsements::list_endorsements_by_subject(&self.pool, subject_id).await
    }

    async fn revoke_endorsement(
        &self,
        endorser_id: Uuid,
        subject_id: Uuid,
        topic: &str,
    ) -> Result<(), EndorsementRepoError> {
        endorsements::revoke_endorsement(&self.pool, endorser_id, subject_id, topic).await
    }

    async fn count_active_trust_endorsements_by(
        &self,
        endorser_id: Uuid,
    ) -> Result<i64, EndorsementRepoError> {
        endorsements::count_active_trust_endorsements_by(&self.pool, endorser_id).await
    }

    async fn link_external_identity(
        &self,
        account_id: Uuid,
        provider: &str,
        provider_subject: &str,
    ) -> Result<ExternalIdentityRecord, ExternalIdentityRepoError> {
        external_identities::link_external_identity(
            &self.pool,
            account_id,
            provider,
            provider_subject,
        )
        .await
    }

    async fn get_external_identity_by_provider(
        &self,
        provider: &str,
        provider_subject: &str,
    ) -> Result<ExternalIdentityRecord, ExternalIdentityRepoError> {
        external_identities::get_external_identity_by_provider(
            &self.pool,
            provider,
            provider_subject,
        )
        .await
    }
}

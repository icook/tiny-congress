//! Brand ethics seeding — orchestrates Phase 1 (company curation) and
//! Phase 2 (per-company evidence generation) to populate the Brand Ethics room.

use anyhow::Context;
use uuid::Uuid;

use crate::sim::{
    client::{EvidenceBody, SimClient},
    config::SimConfig,
    identity::SimAccount,
    llm::{self, Usage},
};

/// Fixed dimensions for all brand ethics polls.
pub const DIMENSIONS: &[(&str, &str, &str)] = &[
    ("Labor Practices", "Exploitative", "Exemplary"),
    ("Environmental Impact", "Destructive", "Regenerative"),
    ("Consumer Trust", "Deceptive", "Transparent"),
    ("Community Impact", "Extractive", "Invested"),
    ("Corporate Governance", "Self-Serving", "Accountable"),
];

pub const ROOM_NAME: &str = "Brand Ethics";

/// Seed the brand ethics room. Idempotent — skips if room already has content.
/// Returns total LLM token usage.
///
/// # Errors
///
/// Returns an error if any API call fails.
pub async fn seed_brand_ethics(
    http: &reqwest::Client,
    client: &SimClient,
    config: &SimConfig,
    admin: &SimAccount,
    verifier_account_id: Option<Uuid>,
) -> Result<Usage, anyhow::Error> {
    let mut usage = Usage::default();

    // 1. Check if room already exists
    let rooms = client.list_rooms().await?;
    let existing = rooms.iter().find(|r| r.name == ROOM_NAME);

    if existing.is_some() {
        tracing::info!("Brand Ethics room already exists, skipping creation");
        return Ok(usage);
    }

    // 2. Phase 1: Curate companies via LLM
    tracing::info!(
        count = config.company_count,
        "Phase 1: curating companies..."
    );
    let (curation, curation_usage) =
        llm::generate_company_curation(http, config, config.company_count).await?;
    usage += curation_usage;
    tracing::info!(companies = curation.companies.len(), "companies curated");

    // 3. Create room with identity_verified constraint
    let constraint_config = verifier_account_id.map(|id| serde_json::json!({"verifier_ids": [id]}));
    let room = client
        .create_room(
            admin,
            ROOM_NAME,
            "Rate S&P 500 companies on ethical dimensions. How do the companies that touch your daily life actually behave?",
            "identity_verified",
            "identity_verified",
            constraint_config.as_ref(),
            Some(config.poll_duration_secs),
        )
        .await
        .context("failed to create Brand Ethics room")?;
    tracing::info!(room_id = %room.id, "Brand Ethics room created");

    // 4. For each company: create poll, dimensions, evidence
    for (i, company) in curation.companies.iter().enumerate() {
        tracing::info!(
            company = %company.name,
            ticker = %company.ticker,
            position = i,
            "seeding company poll..."
        );

        // Phase 2: Generate evidence for this company
        let (evidence, ev_usage) =
            llm::generate_company_evidence(http, config, &company.name, &company.ticker).await?;
        usage += ev_usage;

        // Create poll (question = company name, description = relevance hook)
        let poll = client
            .create_poll(admin, room.id, &company.name, &evidence.relevance_hook)
            .await
            .context("failed to create company poll")?;

        // Create 5 fixed dimensions and attach evidence
        for (sort_order, (dim_name, min_label, max_label)) in DIMENSIONS.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            let order = sort_order as i32;
            let dim = client
                .add_dimension(
                    admin,
                    room.id,
                    poll.id,
                    dim_name,
                    "", // no description needed — evidence cards provide context
                    0.0,
                    1.0,
                    order,
                    Some(min_label),
                    Some(max_label),
                )
                .await
                .context("failed to create dimension")?;

            // Insert evidence cards for this dimension
            if let Some(dim_evidence) = evidence.dimensions.get(*dim_name) {
                let mut cards: Vec<EvidenceBody> = Vec::new();
                for claim in &dim_evidence.pro {
                    cards.push(EvidenceBody {
                        stance: "pro".to_string(),
                        claim: claim.clone(),
                        source: None,
                    });
                }
                for claim in &dim_evidence.con {
                    cards.push(EvidenceBody {
                        stance: "con".to_string(),
                        claim: claim.clone(),
                        source: None,
                    });
                }
                if !cards.is_empty() {
                    client
                        .add_evidence(admin, room.id, poll.id, dim.id, &cards)
                        .await
                        .context("failed to insert evidence")?;
                }
            }
        }
        tracing::info!(company = %company.name, "company poll seeded");
    }

    tracing::info!(
        companies = curation.companies.len(),
        "Brand Ethics room fully seeded"
    );
    Ok(usage)
}

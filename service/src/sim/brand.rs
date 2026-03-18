//! Brand ethics seeding — orchestrates Phase 1 (company curation) and
//! Phase 2 (per-company evidence generation) to populate the Brand Ethics room.

use anyhow::Context;
use serde::{Deserialize, Serialize};
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

/// Check if the Brand Ethics room needs refilling and do so if needed.
///
/// Returns `Ok(Usage::default())` immediately if the room is not in the
/// capacity list (meaning it still has active or draft polls). If the room
/// does appear, all polls are reset to draft and their evidence is
/// regenerated via LLM.
///
/// # Errors
///
/// Returns an error if any API call or LLM request fails.
pub async fn refill_if_needed(
    http: &reqwest::Client,
    client: &SimClient,
    config: &SimConfig,
    admin: &SimAccount,
) -> Result<Usage, anyhow::Error> {
    let mut usage = Usage::default();

    // Capacity list only includes rooms with poll_duration_secs set and no
    // active/draft polls — if Brand Ethics appears here, the cycle is done.
    let capacity_rooms = client.get_capacity().await?;
    let brand_room = capacity_rooms.iter().find(|r| r.name == ROOM_NAME);

    let Some(brand_room) = brand_room else {
        tracing::info!("Brand Ethics room has active content, no refill needed");
        return Ok(usage);
    };

    tracing::info!(
        room_id = %brand_room.id,
        "Brand Ethics room needs refill — starting ring buffer reset"
    );

    // Get all polls in the room (all should be closed at this point)
    let polls = client.list_polls(brand_room.id).await?;
    tracing::info!(poll_count = polls.len(), "polls to reset");

    for poll in &polls {
        // Delete stale evidence before regenerating
        client
            .delete_poll_evidence(admin, brand_room.id, poll.id)
            .await?;

        // Reset poll status to draft so the lifecycle queue can reactivate it
        client.reset_poll(admin, brand_room.id, poll.id).await?;

        // question IS the company name (see seed_brand_ethics)
        let (evidence, ev_usage) =
            llm::generate_company_evidence(http, config, &poll.question, "").await?;
        usage += ev_usage;

        // Fetch dimensions so we can attach evidence per dimension
        let detail = client.get_poll_detail(brand_room.id, poll.id).await?;

        for dim in &detail.dimensions {
            if let Some(dim_evidence) = evidence.dimensions.get(&dim.name) {
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
                        .add_evidence(admin, brand_room.id, poll.id, dim.id, &cards)
                        .await?;
                }
            }
        }

        tracing::info!(company = %poll.question, "poll reset with fresh evidence");
    }

    tracing::info!("ring buffer reset complete — lifecycle will activate first poll");
    Ok(usage)
}

// ─── Dry-run output types ──────────────────────────────────────────────────

#[derive(Serialize)]
struct DryRunOutput {
    companies: Vec<DryRunCompany>,
    token_usage: Usage,
}

#[derive(Serialize)]
struct DryRunCompany {
    ticker: String,
    name: String,
    relevance_hook: String,
    dimensions: Vec<DryRunDimension>,
}

#[derive(Serialize)]
struct DryRunDimension {
    name: String,
    min_label: String,
    max_label: String,
    evidence: Vec<DryRunEvidence>,
}

#[derive(Serialize)]
struct DryRunEvidence {
    stance: String,
    claim: String,
}

/// A model+search configuration pair for battery testing.
#[derive(Debug, Deserialize)]
pub struct BatteryConfig {
    pub model: String,
    pub search: bool,
}

#[derive(Serialize)]
struct BatteryOutput {
    company: String,
    ticker: String,
    runs: Vec<BatteryRun>,
    total_cost_usd: Option<f64>,
}

#[derive(Serialize)]
struct BatteryRun {
    model: String,
    search: bool,
    token_usage: Usage,
    cost_usd: Option<f64>,
    duration_secs: f64,
    generation_id: String,
    evidence: BatteryEvidence,
    raw_response: String,
}

#[derive(Serialize)]
struct BatteryEvidence {
    relevance_hook: String,
    dimensions: Vec<DryRunDimension>,
}

/// Run a battery of model+search combinations for a single company.
/// Writes a comparison JSON file for side-by-side evaluation.
///
/// `pairs` is a list of `(model, search_enabled)` to test.
///
/// # Errors
///
/// Returns an error if any LLM call fails or the output file can't be written.
#[allow(clippy::too_many_lines)]
pub async fn battery(
    http: &reqwest::Client,
    api_key: &str,
    company_name: &str,
    ticker: &str,
    pairs: &[BatteryConfig],
) -> Result<(), anyhow::Error> {
    let mut runs = Vec::with_capacity(pairs.len());

    for (i, pair) in pairs.iter().enumerate() {
        tracing::info!(
            run = i + 1,
            total = pairs.len(),
            model = %pair.model,
            search = pair.search,
            company = company_name,
            "running battery..."
        );

        let start = std::time::Instant::now();
        let result = llm::generate_company_evidence_with_overrides(
            http,
            api_key,
            &pair.model,
            pair.search,
            company_name,
            ticker,
        )
        .await;
        let duration_secs = start.elapsed().as_secs_f64();

        match result {
            Ok((evidence, usage, raw, generation_id)) => {
                // Fetch cost from OpenRouter generation stats (async, best-effort)
                let cost_usd = llm::get_generation_cost(http, api_key, &generation_id)
                    .await
                    .unwrap_or(None);

                tracing::info!(
                    model = %pair.model,
                    cost_usd = ?cost_usd,
                    duration_secs,
                    generation_id = %generation_id,
                    "battery run complete"
                );

                let dimensions: Vec<DryRunDimension> = DIMENSIONS
                    .iter()
                    .map(|(dim_name, min_label, max_label)| {
                        let evidence_cards = evidence
                            .dimensions
                            .get(*dim_name)
                            .map(|de| {
                                let mut cards = Vec::new();
                                for claim in &de.pro {
                                    cards.push(DryRunEvidence {
                                        stance: "pro".to_string(),
                                        claim: claim.clone(),
                                    });
                                }
                                for claim in &de.con {
                                    cards.push(DryRunEvidence {
                                        stance: "con".to_string(),
                                        claim: claim.clone(),
                                    });
                                }
                                cards
                            })
                            .unwrap_or_default();

                        DryRunDimension {
                            name: (*dim_name).to_string(),
                            min_label: (*min_label).to_string(),
                            max_label: (*max_label).to_string(),
                            evidence: evidence_cards,
                        }
                    })
                    .collect();

                runs.push(BatteryRun {
                    model: pair.model.clone(),
                    search: pair.search,
                    token_usage: usage,
                    cost_usd,
                    duration_secs,
                    generation_id,
                    evidence: BatteryEvidence {
                        relevance_hook: evidence.relevance_hook,
                        dimensions,
                    },
                    raw_response: raw,
                });
            }
            Err(e) => {
                tracing::error!(
                    model = %pair.model,
                    search = pair.search,
                    error = %e,
                    duration_secs,
                    "battery run failed"
                );
            }
        }
    }

    let total_cost_usd: Option<f64> = {
        let costs: Vec<f64> = runs.iter().filter_map(|r| r.cost_usd).collect();
        if costs.is_empty() {
            None
        } else {
            Some(costs.iter().sum())
        }
    };

    let output = BatteryOutput {
        company: company_name.to_string(),
        ticker: ticker.to_string(),
        runs,
        total_cost_usd,
    };

    let json =
        serde_json::to_string_pretty(&output).context("failed to serialize battery output")?;
    let path = format!(
        "battery_{}.json",
        company_name.to_lowercase().replace(' ', "_")
    );
    std::fs::write(&path, &json).context("failed to write battery output")?;

    tracing::info!(path = %path, "battery complete — output written");
    Ok(())
}

/// Run LLM generation only, no API calls. Writes results to a JSON file
/// for prompt iteration and quality review.
///
/// # Errors
///
/// Returns an error if any LLM call fails or the output file can't be written.
pub async fn dry_run(http: &reqwest::Client, config: &SimConfig) -> Result<(), anyhow::Error> {
    let mut total_usage = Usage::default();

    // Phase 1: Curate companies
    tracing::info!(
        count = config.company_count,
        "Phase 1: curating companies..."
    );
    let (curation, curation_usage) =
        llm::generate_company_curation(http, config, config.company_count).await?;
    total_usage += curation_usage;

    tracing::info!(companies = curation.companies.len(), "companies curated");

    // Phase 2: Generate evidence for each company
    let mut companies = Vec::with_capacity(curation.companies.len());

    for company in &curation.companies {
        tracing::info!(
            company = %company.name,
            ticker = %company.ticker,
            "Phase 2: generating evidence..."
        );

        let (evidence, ev_usage) =
            llm::generate_company_evidence(http, config, &company.name, &company.ticker).await?;
        total_usage += ev_usage;

        let dimensions: Vec<DryRunDimension> = DIMENSIONS
            .iter()
            .map(|(dim_name, min_label, max_label)| {
                let evidence_cards = evidence
                    .dimensions
                    .get(*dim_name)
                    .map(|de| {
                        let mut cards = Vec::new();
                        for claim in &de.pro {
                            cards.push(DryRunEvidence {
                                stance: "pro".to_string(),
                                claim: claim.clone(),
                            });
                        }
                        for claim in &de.con {
                            cards.push(DryRunEvidence {
                                stance: "con".to_string(),
                                claim: claim.clone(),
                            });
                        }
                        cards
                    })
                    .unwrap_or_default();

                DryRunDimension {
                    name: (*dim_name).to_string(),
                    min_label: (*min_label).to_string(),
                    max_label: (*max_label).to_string(),
                    evidence: evidence_cards,
                }
            })
            .collect();

        companies.push(DryRunCompany {
            ticker: company.ticker.clone(),
            name: company.name.clone(),
            relevance_hook: evidence.relevance_hook.clone(),
            dimensions,
        });
    }

    let output = DryRunOutput {
        companies,
        token_usage: total_usage,
    };

    let json = serde_json::to_string_pretty(&output).context("failed to serialize output")?;
    let path = "brand_ethics_dry_run.json";
    std::fs::write(path, &json).context("failed to write output file")?;

    tracing::info!(
        path = path,
        companies = output.companies.len(),
        total_tokens = total_usage.total_tokens,
        "dry run complete — output written"
    );

    Ok(())
}

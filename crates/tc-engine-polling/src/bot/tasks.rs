//! Concrete bot task execution logic.
//!
//! Each function in this module corresponds to a bot task variant and runs
//! the full LLM + Exa pipeline, persisting results to the database.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::time::Instant;

use anyhow::Context as _;
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::repo::bot_traces::TraceStep;
use crate::repo::evidence::NewEvidence;
use crate::repo::{bot_traces, evidence as evidence_repo, polls};

use super::config::BotConfig;
use super::worker::BotWorkerConfig;
use crate::repo::pgmq::BotTask;

// ─── Constants ───────────────────────────────────────────────────────────────

/// Fixed ethical dimensions for brand ethics polls.
const DIMENSIONS: &[(&str, &str, &str)] = &[
    ("Labor Practices", "Exploitative", "Exemplary"),
    ("Environmental Impact", "Destructive", "Regenerative"),
    ("Consumer Trust", "Deceptive", "Transparent"),
    ("Community Impact", "Extractive", "Invested"),
    ("Corporate Governance", "Self-Serving", "Accountable"),
];

const EXA_SYNTHESIS_SYSTEM: &str = r"You are a balanced research analyst extracting structured evidence from search results. For each of the 5 ethical dimensions, extract 2-3 specific, factual pro and con claims directly supported by the search results provided. Each claim must be one sentence and grounded in the sources — do not fabricate claims. If a dimension has weak search coverage, provide fewer claims rather than speculating.";

// ─── Local LLM response types ────────────────────────────────────────────────

/// Top-level LLM response for company evidence synthesis.
#[derive(Debug, Deserialize)]
struct CompanyEvidence {
    relevance_hook: String,
    dimensions: HashMap<String, DimensionEvidence>,
}

/// Pro/con evidence for a single ethical dimension.
#[derive(Debug, Deserialize)]
struct DimensionEvidence {
    pro: Vec<String>,
    con: Vec<String>,
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Load `engine_config` JSON for a room.
async fn get_room_engine_config(pool: &PgPool, room_id: Uuid) -> anyhow::Result<serde_json::Value> {
    let row: (serde_json::Value,) =
        sqlx::query_as("SELECT engine_config FROM rooms__rooms WHERE id = $1")
            .bind(room_id)
            .fetch_one(pool)
            .await
            .with_context(|| format!("loading engine_config for room {room_id}"))?;
    Ok(row.0)
}

/// Determine which LLM model to use: room override → worker default.
fn resolve_model(bot_config: Option<&BotConfig>, worker_config: &BotWorkerConfig) -> String {
    bot_config
        .and_then(|bc| bc.model.clone())
        .unwrap_or_else(|| worker_config.default_model.clone())
}

/// Build the synthesis messages for the LLM, same pattern as the sim.
fn build_synthesis_messages(
    company_name: &str,
    ticker: &str,
    search_context: &str,
) -> Vec<tc_llm::ChatMessage> {
    let user_content = format!(
        r#"Below are search results about {company_name} ({ticker}) organized by ethical dimension.
Extract structured evidence cards from these results.

{search_context}

Respond with ONLY valid JSON matching this schema:
{{
  "relevance_hook": "One sentence explaining how this company affects daily life.",
  "dimensions": {{
    "Labor Practices": {{
      "pro": ["Factual positive claim with source detail."],
      "con": ["Factual negative claim with source detail."]
    }},
    "Environmental Impact": {{
      "pro": ["Factual positive claim."],
      "con": ["Factual negative claim."]
    }},
    "Consumer Trust": {{
      "pro": ["Factual positive claim."],
      "con": ["Factual negative claim."]
    }},
    "Community Impact": {{
      "pro": ["Factual positive claim."],
      "con": ["Factual negative claim."]
    }},
    "Corporate Governance": {{
      "pro": ["Factual positive claim."],
      "con": ["Factual negative claim."]
    }}
  }}
}}"#
    );

    vec![
        tc_llm::ChatMessage {
            role: "system".to_string(),
            content: EXA_SYNTHESIS_SYSTEM.to_string(),
        },
        tc_llm::ChatMessage {
            role: "user".to_string(),
            content: user_content,
        },
    ]
}

/// Run parallel Exa searches for all 5 dimensions and return a formatted
/// search context string + individual responses for tracing.
async fn run_exa_searches(
    pool: &PgPool,
    http: &reqwest::Client,
    config: &BotWorkerConfig,
    company_name: &str,
    trace_id: Uuid,
) -> anyhow::Result<String> {
    let mut join_set = tokio::task::JoinSet::new();

    for (dim_name, _, _) in DIMENSIONS {
        let http = http.clone();
        let api_key = config.exa_api_key.clone();
        let base_url = config.exa_base_url.clone();
        let company = company_name.to_string();
        let dim = (*dim_name).to_string();

        join_set.spawn(async move {
            let query = format!("{company} {dim}");
            let start = Instant::now();
            let result = tc_llm::exa_search(&http, &api_key, &base_url, &query, 5).await;
            let elapsed = start.elapsed();
            (dim, query, result, elapsed)
        });
    }

    // Collect results, appending a trace step for each search
    let mut dim_results: HashMap<String, tc_llm::SearchResponse> = HashMap::new();
    while let Some(join_result) = join_set.join_next().await {
        let (dim_name, query, search_result, elapsed) =
            join_result.with_context(|| "Exa search task panicked")?;

        match search_result {
            Ok(response) => {
                tracing::info!(
                    dimension = %dim_name,
                    results = response.results.len(),
                    company_name,
                    "exa_search complete"
                );

                #[allow(clippy::cast_possible_truncation)]
                let latency_ms = elapsed.as_millis() as u64;
                let step = TraceStep {
                    step_type: "exa_search".to_string(),
                    model: None,
                    query: Some(query.clone()),
                    prompt_tokens: None,
                    completion_tokens: None,
                    latency_ms,
                    cost_usd: 0.0,
                    cache: serde_json::to_value(&response.cache).unwrap_or_default(),
                    output_summary: format!(
                        "{} results for '{}'",
                        response.results.len(),
                        dim_name
                    ),
                };
                if let Err(e) = bot_traces::append_step(pool, trace_id, &step).await {
                    tracing::warn!(error = %e, "failed to append exa trace step");
                }

                dim_results.insert(dim_name, response);
            }
            Err(e) => {
                tracing::warn!(
                    dimension = %dim_name,
                    error = %e,
                    "exa search failed for dimension, continuing"
                );
            }
        }
    }

    // Build search context in dimension order
    let mut search_context = String::new();
    for (dim_name, _, _) in DIMENSIONS {
        if let Some(response) = dim_results.get(*dim_name) {
            write!(search_context, "\n## {dim_name}\n\n").ok();
            for r in &response.results {
                let title = if r.title.is_empty() {
                    "(no title)"
                } else {
                    &r.title
                };
                write!(
                    search_context,
                    "### [{title}]({url})\n{text}\n\n",
                    url = r.url,
                    text = r.text,
                )
                .ok();
            }
        }
    }

    if search_context.is_empty() {
        anyhow::bail!("all Exa searches failed for {company_name} — cannot synthesize evidence");
    }

    Ok(search_context)
}

/// Synthesize evidence from search context via LLM, appending a trace step.
#[allow(clippy::too_many_arguments)]
async fn synthesize_evidence(
    pool: &PgPool,
    http: &reqwest::Client,
    config: &BotWorkerConfig,
    model: &str,
    company_name: &str,
    ticker: &str,
    search_context: &str,
    trace_id: Uuid,
) -> anyhow::Result<CompanyEvidence> {
    let messages = build_synthesis_messages(company_name, ticker, search_context);

    let start = Instant::now();
    let completion = tc_llm::chat_completion(
        http,
        &config.llm_api_key,
        &config.llm_base_url,
        model,
        messages,
        true,
        Some(0.3),
    )
    .await
    .context("LLM synthesis call failed")?;
    let elapsed = start.elapsed();

    #[allow(clippy::cast_possible_truncation)]
    let latency_ms = elapsed.as_millis() as u64;
    let step = TraceStep {
        step_type: "llm_synthesis".to_string(),
        model: Some(model.to_string()),
        query: None,
        prompt_tokens: Some(completion.usage.prompt_tokens),
        completion_tokens: Some(completion.usage.completion_tokens),
        latency_ms,
        cost_usd: completion.usage.cost.unwrap_or(0.0),
        cache: serde_json::to_value(&completion.cache).unwrap_or_default(),
        output_summary: "evidence synthesis".to_string(),
    };
    if let Err(e) = bot_traces::append_step(pool, trace_id, &step).await {
        tracing::warn!(error = %e, "failed to append llm trace step");
    }

    let json_str = tc_llm::extract_json(&completion.content);
    let evidence: CompanyEvidence = serde_json::from_str(json_str).with_context(|| {
        format!(
            "failed to parse synthesis JSON\nraw: {}",
            completion.content
        )
    })?;

    Ok(evidence)
}

/// Insert dimensions + evidence into the database for a poll.
async fn insert_dimensions_and_evidence(
    pool: &PgPool,
    poll_id: Uuid,
    evidence: &CompanyEvidence,
) -> anyhow::Result<()> {
    for (sort_order, (dim_name, min_label, max_label)) in DIMENSIONS.iter().enumerate() {
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let order = sort_order as i32;
        let dim = polls::create_dimension(
            pool,
            poll_id,
            dim_name,
            Some(""),
            0.0,
            1.0,
            order,
            Some(min_label),
            Some(max_label),
        )
        .await
        .with_context(|| format!("creating dimension '{dim_name}'"))?;

        if let Some(dim_ev) = evidence.dimensions.get(*dim_name) {
            let mut items: Vec<NewEvidence<'_>> = Vec::new();
            for claim in &dim_ev.pro {
                items.push(NewEvidence {
                    stance: "pro",
                    claim,
                    source: None,
                });
            }
            for claim in &dim_ev.con {
                items.push(NewEvidence {
                    stance: "con",
                    claim,
                    source: None,
                });
            }
            if !items.is_empty() {
                evidence_repo::insert_evidence(pool, dim.id, &items)
                    .await
                    .with_context(|| format!("inserting evidence for dimension '{dim_name}'"))?;
            }
        }
    }
    Ok(())
}

// ─── Public task handlers ────────────────────────────────────────────────────

/// Research a company and create a new poll with evidence.
///
/// Params:
/// - `company` (String): company name
/// - `ticker` (String, optional): ticker symbol
///
/// Returns the new poll ID on success.
///
/// # Errors
///
/// Returns an error if Exa searches all fail, the LLM call fails, or any
/// database operation fails.
pub async fn research_company(
    pool: &PgPool,
    http: &reqwest::Client,
    config: &BotWorkerConfig,
    task: &BotTask,
    trace_id: Uuid,
) -> anyhow::Result<Option<Uuid>> {
    let company = task
        .params
        .get("company")
        .and_then(|v| v.as_str())
        .context("missing 'company' param")?
        .to_string();

    let ticker = task
        .params
        .get("ticker")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Resolve model (room override → worker default)
    let engine_config = get_room_engine_config(pool, task.room_id).await?;
    let bot_config = BotConfig::from_engine_config(&engine_config);
    let model = resolve_model(bot_config.as_ref(), config);

    tracing::info!(
        room_id = %task.room_id,
        company = %company,
        ticker = %ticker,
        model = %model,
        "research_company: starting"
    );

    // Parallel Exa searches
    let search_context = run_exa_searches(pool, http, config, &company, trace_id).await?;

    // LLM synthesis
    let evidence = synthesize_evidence(
        pool,
        http,
        config,
        &model,
        &company,
        &ticker,
        &search_context,
        trace_id,
    )
    .await?;

    // Create poll
    let poll = polls::create_poll(
        pool,
        task.room_id,
        &company,
        Some(&evidence.relevance_hook),
        None,
    )
    .await
    .context("creating poll")?;

    // Insert dimensions + evidence
    insert_dimensions_and_evidence(pool, poll.id, &evidence).await?;

    tracing::info!(
        room_id = %task.room_id,
        poll_id = %poll.id,
        company = %company,
        "research_company: completed"
    );

    Ok(Some(poll.id))
}

/// Regenerate evidence for an existing poll.
///
/// Params:
/// - `poll_id` (Uuid): the poll to regenerate evidence for
///
/// Returns the poll ID on success.
///
/// # Errors
///
/// Returns an error if the poll is not found, Exa searches all fail, the LLM
/// call fails, or any database operation fails.
pub async fn generate_evidence(
    pool: &PgPool,
    http: &reqwest::Client,
    config: &BotWorkerConfig,
    task: &BotTask,
    trace_id: Uuid,
) -> anyhow::Result<Option<Uuid>> {
    let poll_id_str = task
        .params
        .get("poll_id")
        .and_then(|v| v.as_str())
        .context("missing 'poll_id' param")?;

    let poll_id =
        Uuid::parse_str(poll_id_str).with_context(|| format!("invalid poll_id: {poll_id_str}"))?;

    // Load poll to get company name (question field)
    let poll = polls::get_poll(pool, poll_id)
        .await
        .with_context(|| format!("loading poll {poll_id}"))?;

    let company = poll.question.clone();
    let ticker = String::new();

    // Resolve model
    let engine_config = get_room_engine_config(pool, task.room_id).await?;
    let bot_config = BotConfig::from_engine_config(&engine_config);
    let model = resolve_model(bot_config.as_ref(), config);

    tracing::info!(
        room_id = %task.room_id,
        poll_id = %poll_id,
        company = %company,
        model = %model,
        "generate_evidence: starting"
    );

    // Parallel Exa searches
    let search_context = run_exa_searches(pool, http, config, &company, trace_id).await?;

    // LLM synthesis
    let evidence = synthesize_evidence(
        pool,
        http,
        config,
        &model,
        &company,
        &ticker,
        &search_context,
        trace_id,
    )
    .await?;

    // Load dimensions, then delete old evidence and insert new evidence.
    // Delete is deferred until after LLM success to avoid a data-loss window
    // if the synthesis call fails.
    let dimensions = polls::list_dimensions(pool, poll_id)
        .await
        .with_context(|| format!("listing dimensions for poll {poll_id}"))?;

    evidence_repo::delete_evidence_for_poll(pool, poll_id)
        .await
        .with_context(|| format!("deleting evidence for poll {poll_id}"))?;

    for dim in &dimensions {
        if let Some(dim_ev) = evidence.dimensions.get(&dim.name) {
            let mut items: Vec<NewEvidence<'_>> = Vec::new();
            for claim in &dim_ev.pro {
                items.push(NewEvidence {
                    stance: "pro",
                    claim,
                    source: None,
                });
            }
            for claim in &dim_ev.con {
                items.push(NewEvidence {
                    stance: "con",
                    claim,
                    source: None,
                });
            }
            if !items.is_empty() {
                evidence_repo::insert_evidence(pool, dim.id, &items)
                    .await
                    .with_context(|| format!("inserting evidence for dimension '{}'", dim.name))?;
            }
        }
    }

    tracing::info!(
        room_id = %task.room_id,
        poll_id = %poll_id,
        company = %company,
        "generate_evidence: completed"
    );

    Ok(Some(poll_id))
}

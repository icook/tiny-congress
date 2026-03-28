//! Concrete bot task execution logic.
//!
//! Each function in this module corresponds to a bot task variant and runs
//! the full LLM + Exa pipeline, persisting results to the database.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::time::Instant;

use anyhow::Context as _;
use sqlx::PgPool;
use uuid::Uuid;

use tc_llm::{build_synthesis_messages, CompanyEvidence, DIMENSIONS};

use crate::repo::bot_traces::TraceStep;
use crate::repo::evidence::NewEvidence;
use crate::repo::{bot_traces, evidence as evidence_repo, polls};

use super::config::BotConfig;
use super::worker::BotWorkerConfig;
use crate::repo::pgmq::BotTask;

const SUGGESTION_QUERY_SYSTEM: &str = "\
You are a research assistant. Given a user's suggestion and the context of a poll, \
generate 2-3 focused web search queries that would find relevant evidence. \
Return JSON: {\"queries\": [\"query1\", \"query2\", ...]}";

const SUGGESTION_SYNTHESIS_SYSTEM: &str = "\
You are a research analyst. Given search results about a topic, extract 2-4 evidence claims. \
Each claim should be a clear factual assertion with a pro or con stance. \
Return JSON: {\"dimension_name\": \"<best fit dimension or new name>\", \
\"evidence\": [{\"stance\": \"pro\"|\"con\", \"claim\": \"...\", \"source\": \"url\"}]}";

#[derive(Debug, serde::Deserialize)]
struct SuggestionQueries {
    queries: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
struct SuggestionEvidence {
    dimension_name: String,
    evidence: Vec<SuggestionEvidenceItem>,
}

#[derive(Debug, serde::Deserialize)]
struct SuggestionEvidenceItem {
    stance: String,
    claim: String,
    source: Option<String>,
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
    let messages = build_synthesis_messages(company_name, ticker, search_context, None);

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
    let position = polls::next_agenda_position(pool, task.room_id)
        .await
        .context("computing agenda position")?;
    let poll = polls::create_poll(
        pool,
        task.room_id,
        &company,
        Some(&evidence.relevance_hook),
        Some(position),
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

/// Claim the next queued suggestion and enqueue a research task for it.
/// Enqueued periodically by the lifecycle scheduler.
///
/// # Errors
///
/// Returns an error if the database query fails or task enqueue fails.
pub async fn process_suggestions(
    pool: &PgPool,
    _http: &reqwest::Client,
    _config: &BotWorkerConfig,
    task: &BotTask,
    _trace_id: Uuid,
) -> anyhow::Result<Option<Uuid>> {
    // Claim next queued suggestion (FIFO, with FOR UPDATE SKIP LOCKED)
    let row: Option<(Uuid, String, Uuid)> = sqlx::query_as(
        r"UPDATE rooms__research_suggestions
         SET status = 'researching', processed_at = now()
         WHERE id = (
             SELECT id FROM rooms__research_suggestions
             WHERE status = 'queued' AND room_id = $1
             ORDER BY created_at ASC
             LIMIT 1
             FOR UPDATE SKIP LOCKED
         )
         RETURNING id, suggestion_text, poll_id",
    )
    .bind(task.room_id)
    .fetch_optional(pool)
    .await?;

    let Some((suggestion_id, suggestion_text, poll_id)) = row else {
        tracing::debug!(room_id = %task.room_id, "no queued suggestions");
        return Ok(None);
    };

    tracing::info!(
        suggestion_id = %suggestion_id,
        room_id = %task.room_id,
        poll_id = %poll_id,
        "claimed suggestion, enqueuing research task"
    );

    // Enqueue the actual research task
    crate::repo::pgmq::send_task(
        pool,
        &BotTask {
            room_id: task.room_id,
            task: "research_suggestion".to_string(),
            params: serde_json::json!({
                "suggestion_id": suggestion_id,
                "suggestion_text": suggestion_text,
                "poll_id": poll_id,
            }),
        },
    )
    .await?;

    Ok(None)
}

/// Research a user suggestion and insert evidence onto the poll's best-fit dimension.
///
/// Params:
/// - `suggestion_id` (Uuid): the suggestion to process
/// - `suggestion_text` (String): the suggestion text
/// - `poll_id` (Uuid): the poll to add evidence to
///
/// Returns the poll ID on success.
///
/// # Errors
///
/// Returns an error if required params are missing/invalid, Exa searches all fail,
/// the LLM calls fail, or any database operation fails.
#[allow(clippy::too_many_lines)]
pub async fn research_suggestion(
    pool: &PgPool,
    http: &reqwest::Client,
    config: &BotWorkerConfig,
    task: &BotTask,
    trace_id: Uuid,
) -> anyhow::Result<Option<Uuid>> {
    let suggestion_id: Uuid = serde_json::from_value(
        task.params
            .get("suggestion_id")
            .ok_or_else(|| anyhow::anyhow!("missing suggestion_id in params"))?
            .clone(),
    )?;

    let suggestion_text = task
        .params
        .get("suggestion_text")
        .and_then(|v| v.as_str())
        .context("missing 'suggestion_text' param")?
        .to_string();

    let poll_id_str = task
        .params
        .get("poll_id")
        .and_then(|v| v.as_str())
        .context("missing 'poll_id' param")?;
    let poll_id =
        Uuid::parse_str(poll_id_str).with_context(|| format!("invalid poll_id: {poll_id_str}"))?;

    // Resolve model (room override → worker default)
    let engine_config = get_room_engine_config(pool, task.room_id).await?;
    let bot_config = BotConfig::from_engine_config(&engine_config);
    let model = resolve_model(bot_config.as_ref(), config);

    // Load poll context and existing dimensions
    let poll = polls::get_poll(pool, poll_id)
        .await
        .with_context(|| format!("loading poll {poll_id}"))?;
    let dimensions = polls::list_dimensions(pool, poll_id)
        .await
        .with_context(|| format!("listing dimensions for poll {poll_id}"))?;

    let dim_names: Vec<&str> = dimensions.iter().map(|d| d.name.as_str()).collect();
    let dim_list = dim_names.join(", ");

    tracing::info!(
        room_id = %task.room_id,
        poll_id = %poll_id,
        suggestion_id = %suggestion_id,
        model = %model,
        "research_suggestion: starting"
    );

    // ── Step 1: LLM generates search queries ──────────────────────────────────
    let query_user = format!(
        "Poll topic: {}\nExisting dimensions: {}\nUser suggestion: {}\n\n\
         Generate 2-3 search queries to find evidence relevant to this suggestion.",
        poll.question, dim_list, suggestion_text
    );
    let query_messages = vec![
        tc_llm::ChatMessage {
            role: "system".to_string(),
            content: SUGGESTION_QUERY_SYSTEM.to_string(),
        },
        tc_llm::ChatMessage {
            role: "user".to_string(),
            content: query_user,
        },
    ];

    let start = Instant::now();
    let query_completion = tc_llm::chat_completion(
        http,
        &config.llm_api_key,
        &config.llm_base_url,
        &model,
        query_messages,
        true,
        Some(0.3),
    )
    .await
    .context("LLM query generation call failed")?;
    let elapsed = start.elapsed();

    #[allow(clippy::cast_possible_truncation)]
    let latency_ms = elapsed.as_millis() as u64;
    let step = TraceStep {
        step_type: "llm_query_gen".to_string(),
        model: Some(model.clone()),
        query: None,
        prompt_tokens: Some(query_completion.usage.prompt_tokens),
        completion_tokens: Some(query_completion.usage.completion_tokens),
        latency_ms,
        cost_usd: query_completion.usage.cost.unwrap_or(0.0),
        cache: serde_json::to_value(&query_completion.cache).unwrap_or_default(),
        output_summary: "suggestion query generation".to_string(),
    };
    if let Err(e) = bot_traces::append_step(pool, trace_id, &step).await {
        tracing::warn!(error = %e, "failed to append llm_query_gen trace step");
    }

    let json_str = tc_llm::extract_json(&query_completion.content);
    let sq: SuggestionQueries = serde_json::from_str(json_str).with_context(|| {
        format!(
            "failed to parse query generation JSON\nraw: {}",
            query_completion.content
        )
    })?;

    if sq.queries.is_empty() {
        anyhow::bail!("LLM returned no search queries for suggestion {suggestion_id}");
    }

    // ── Step 2: Parallel Exa searches ────────────────────────────────────────
    let mut join_set = tokio::task::JoinSet::new();
    for query in &sq.queries {
        let http = http.clone();
        let api_key = config.exa_api_key.clone();
        let base_url = config.exa_base_url.clone();
        let q = query.clone();
        join_set.spawn(async move {
            let start = Instant::now();
            let result = tc_llm::exa_search(&http, &api_key, &base_url, &q, 5).await;
            let elapsed = start.elapsed();
            (q, result, elapsed)
        });
    }

    let mut search_context = String::new();
    while let Some(join_result) = join_set.join_next().await {
        let (query, search_result, elapsed) =
            join_result.with_context(|| "Exa search task panicked")?;

        match search_result {
            Ok(response) => {
                tracing::info!(
                    query = %query,
                    results = response.results.len(),
                    "suggestion exa_search complete"
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
                    output_summary: format!("{} results for '{}'", response.results.len(), query),
                };
                if let Err(e) = bot_traces::append_step(pool, trace_id, &step).await {
                    tracing::warn!(error = %e, "failed to append exa trace step");
                }

                for r in &response.results {
                    write!(
                        search_context,
                        "### [{title}]({url})\n{text}\n\n",
                        title = if r.title.is_empty() {
                            "(no title)"
                        } else {
                            &r.title
                        },
                        url = r.url,
                        text = r.text,
                    )
                    .ok();
                }
            }
            Err(e) => {
                tracing::warn!(query = %query, error = %e, "exa search failed, continuing");
            }
        }
    }

    if search_context.is_empty() {
        // Mark suggestion as failed — no search results available
        sqlx::query(
            "UPDATE rooms__research_suggestions \
             SET status = 'failed', processed_at = now() WHERE id = $1",
        )
        .bind(suggestion_id)
        .execute(pool)
        .await?;
        anyhow::bail!("all Exa searches failed for suggestion {suggestion_id} — marking failed");
    }

    // ── Step 3: LLM synthesizes evidence ─────────────────────────────────────
    let synthesis_user = format!(
        "Poll topic: {}\nExisting dimensions: {}\nUser suggestion: {}\n\n\
         Search results:\n{search_context}\n\n\
         Extract 2-4 evidence claims from these results. \
         Pick the best-fit dimension from the existing list (case-insensitive match), \
         or propose a new dimension name if none fits.",
        poll.question, dim_list, suggestion_text
    );
    let synthesis_messages = vec![
        tc_llm::ChatMessage {
            role: "system".to_string(),
            content: SUGGESTION_SYNTHESIS_SYSTEM.to_string(),
        },
        tc_llm::ChatMessage {
            role: "user".to_string(),
            content: synthesis_user,
        },
    ];

    let start = Instant::now();
    let synthesis_completion = tc_llm::chat_completion(
        http,
        &config.llm_api_key,
        &config.llm_base_url,
        &model,
        synthesis_messages,
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
        model: Some(model.clone()),
        query: None,
        prompt_tokens: Some(synthesis_completion.usage.prompt_tokens),
        completion_tokens: Some(synthesis_completion.usage.completion_tokens),
        latency_ms,
        cost_usd: synthesis_completion.usage.cost.unwrap_or(0.0),
        cache: serde_json::to_value(&synthesis_completion.cache).unwrap_or_default(),
        output_summary: "suggestion evidence synthesis".to_string(),
    };
    if let Err(e) = bot_traces::append_step(pool, trace_id, &step).await {
        tracing::warn!(error = %e, "failed to append llm_synthesis trace step");
    }

    let json_str = tc_llm::extract_json(&synthesis_completion.content);
    let se: SuggestionEvidence = serde_json::from_str(json_str).with_context(|| {
        format!(
            "failed to parse synthesis JSON\nraw: {}",
            synthesis_completion.content
        )
    })?;

    // ── Step 4: Resolve or create dimension ──────────────────────────────────
    let target_dim = dimensions
        .iter()
        .find(|d| d.name.to_lowercase() == se.dimension_name.to_lowercase())
        .cloned();

    let dim = if let Some(existing) = target_dim {
        existing
    } else {
        // Determine sort_order for the new dimension
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let sort_order = dimensions.len() as i32;
        polls::create_dimension(
            pool,
            poll_id,
            &se.dimension_name,
            Some(""),
            0.0,
            1.0,
            sort_order,
            None,
            None,
        )
        .await
        .with_context(|| format!("creating dimension '{}'", se.dimension_name))?
    };

    // ── Step 5: Insert evidence ───────────────────────────────────────────────
    let items: Vec<NewEvidence<'_>> = se
        .evidence
        .iter()
        .map(|ev| NewEvidence {
            stance: ev.stance.as_str(),
            claim: ev.claim.as_str(),
            source: ev.source.as_deref(),
        })
        .collect();

    if !items.is_empty() {
        evidence_repo::insert_evidence(pool, dim.id, &items)
            .await
            .with_context(|| format!("inserting evidence for dimension '{}'", dim.name))?;
    }

    // ── Step 6: Fetch inserted evidence IDs ──────────────────────────────────
    // insert_evidence returns row count not IDs, so query for the most recent
    // evidence on this dimension to recover the IDs.
    #[allow(clippy::cast_possible_wrap)]
    let evidence_limit = items.len() as i64;
    let inserted_ids: Vec<(Uuid,)> = sqlx::query_as(
        r"SELECT id FROM rooms__poll_evidence
          WHERE dimension_id = $1
          ORDER BY created_at DESC
          LIMIT $2",
    )
    .bind(dim.id)
    .bind(evidence_limit)
    .fetch_all(pool)
    .await
    .context("fetching inserted evidence IDs")?;

    let evidence_ids: Vec<Uuid> = inserted_ids.into_iter().map(|(id,)| id).collect();

    // ── Step 7: Update suggestion with evidence IDs ───────────────────────────
    sqlx::query(
        "UPDATE rooms__research_suggestions \
         SET status = 'completed', processed_at = now(), evidence_ids = $1 \
         WHERE id = $2",
    )
    .bind(&evidence_ids)
    .bind(suggestion_id)
    .execute(pool)
    .await
    .context("updating suggestion status to completed")?;

    tracing::info!(
        room_id = %task.room_id,
        poll_id = %poll_id,
        suggestion_id = %suggestion_id,
        evidence_count = evidence_ids.len(),
        dimension = %dim.name,
        "research_suggestion: completed"
    );

    Ok(Some(poll_id))
}

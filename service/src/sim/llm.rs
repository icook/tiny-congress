//! LLM response types, Exa search, and `OpenRouter` client for generating sim content.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::ops::AddAssign;

use serde::{Deserialize, Serialize};

use super::config::SimConfig;

// ---------------------------------------------------------------------------
// Public response types – deserialized from LLM JSON output
// ---------------------------------------------------------------------------

/// Top-level LLM response containing generated rooms.
#[derive(Debug, Clone, Deserialize)]
pub struct SimContent {
    pub rooms: Vec<SimRoom>,
}

/// A generated room with its polls.
#[derive(Debug, Clone, Deserialize)]
pub struct SimRoom {
    pub name: String,
    pub description: String,
    pub polls: Vec<SimPoll>,
}

/// A generated poll with its dimensions.
#[derive(Debug, Clone, Deserialize)]
pub struct SimPoll {
    pub question: String,
    pub description: String,
    pub dimensions: Vec<SimDimension>,
}

/// A single dimension for opinion-space voting.
#[derive(Debug, Clone, Deserialize)]
pub struct SimDimension {
    pub name: String,
    pub description: String,
    pub min: f32,
    pub max: f32,
    pub min_label: Option<String>,
    pub max_label: Option<String>,
}

/// Token usage from a single `OpenRouter` API call.
#[derive(Debug, Default, Clone, Copy, Deserialize, Serialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

impl AddAssign for Usage {
    fn add_assign(&mut self, rhs: Self) {
        self.prompt_tokens += rhs.prompt_tokens;
        self.completion_tokens += rhs.completion_tokens;
        self.total_tokens += rhs.total_tokens;
    }
}

// ---------------------------------------------------------------------------
// OpenRouter API types (private)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    plugins: Vec<Plugin>,
}

#[derive(Debug, Serialize)]
struct Plugin {
    id: String,
}

/// A single chat message in the `OpenRouter` request/response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
struct ResponseFormat {
    r#type: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    id: String,
    choices: Vec<Choice>,
    usage: Option<Usage>,
}

/// Cost and metadata returned by the `OpenRouter` generation stats endpoint.
#[derive(Debug, Deserialize)]
struct GenerationStats {
    /// Total cost in USD for this generation (tokens + search fees).
    total_cost: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChoiceMessage {
    content: String,
}

// ---------------------------------------------------------------------------
// Exa API types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ExaSearchRequest {
    query: String,
    num_results: u32,
    r#type: String,
    contents: ExaContentsOptions,
}

#[derive(Debug, Serialize)]
struct ExaContentsOptions {
    text: ExaTextOptions,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExaTextOptions {
    max_characters: u32,
}

#[derive(Debug, Deserialize)]
struct ExaSearchResponse {
    results: Vec<ExaResult>,
}

#[derive(Debug, Deserialize)]
struct ExaResult {
    url: String,
    title: Option<String>,
    text: Option<String>,
}

// ---------------------------------------------------------------------------
// Public functions
// ---------------------------------------------------------------------------

const USER_PROMPT_TEMPLATE: &str = r#"Generate exactly {N} community governance rooms. Each room must have 2-3 polls, and each poll must have 3-5 dimensions.

Respond with ONLY valid JSON matching this schema:
{
  "rooms": [
    {
      "name": "Room Name",
      "description": "Room description",
      "polls": [
        {
          "question": "Poll question?",
          "description": "Poll description",
          "dimensions": [
            {
              "name": "Dimension Name",
              "description": "What this dimension measures",
              "min": 0.0,
              "max": 10.0,
              "min_label": "Low end label",
              "max_label": "High end label"
            }
          ]
        }
      ]
    }
  ]
}"#;

/// Build the system and user messages for the LLM request.
#[must_use]
pub fn build_messages(config: &SimConfig, rooms_needed: usize) -> Vec<ChatMessage> {
    let user_content = USER_PROMPT_TEMPLATE.replace("{N}", &rooms_needed.to_string());

    vec![
        ChatMessage {
            role: "system".to_string(),
            content: config.system_prompt.clone(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: user_content,
        },
    ]
}

/// Return deterministic mock content for CI testing without `OpenRouter`.
#[must_use]
pub fn mock_content(rooms_needed: usize) -> SimContent {
    let rooms = (0..rooms_needed)
        .map(|i| SimRoom {
            name: format!("Mock Room {}", i + 1),
            description: format!("A mock room for CI testing (room {})", i + 1),
            polls: vec![
                SimPoll {
                    question: format!("Should we improve mock topic {}A?", i + 1),
                    description: "A mock poll for pipeline testing".to_string(),
                    dimensions: vec![
                        SimDimension {
                            name: "Impact".to_string(),
                            description: "Expected community impact".to_string(),
                            min: 0.0,
                            max: 10.0,
                            min_label: Some("No impact".to_string()),
                            max_label: Some("Major impact".to_string()),
                        },
                        SimDimension {
                            name: "Feasibility".to_string(),
                            description: "How feasible is this proposal".to_string(),
                            min: 0.0,
                            max: 5.0,
                            min_label: Some("Not feasible".to_string()),
                            max_label: Some("Very feasible".to_string()),
                        },
                    ],
                },
                SimPoll {
                    question: format!("How should we fund mock topic {}B?", i + 1),
                    description: "A second mock poll for pipeline testing".to_string(),
                    dimensions: vec![
                        SimDimension {
                            name: "Cost".to_string(),
                            description: "Estimated cost to implement".to_string(),
                            min: 0.0,
                            max: 10.0,
                            min_label: Some("Low cost".to_string()),
                            max_label: Some("High cost".to_string()),
                        },
                        SimDimension {
                            name: "Priority".to_string(),
                            description: "How urgent is this".to_string(),
                            min: 1.0,
                            max: 5.0,
                            min_label: Some("Low priority".to_string()),
                            max_label: Some("High priority".to_string()),
                        },
                        SimDimension {
                            name: "Public Support".to_string(),
                            description: "Expected level of public support".to_string(),
                            min: 0.0,
                            max: 10.0,
                            min_label: Some("Low support".to_string()),
                            max_label: Some("High support".to_string()),
                        },
                    ],
                },
            ],
        })
        .collect();

    SimContent { rooms }
}

/// Call the `OpenRouter` API to generate sim content, or return mock content
/// if `config.mock_llm` is true.
///
/// Returns the generated content and token usage for this call.
///
/// # Errors
///
/// Returns an error if the HTTP request fails, the response cannot be parsed,
/// or the LLM returns an empty choices array.
pub async fn generate_content(
    client: &reqwest::Client,
    config: &SimConfig,
    rooms_needed: usize,
) -> Result<(SimContent, Usage), anyhow::Error> {
    if config.mock_llm {
        tracing::info!(rooms_needed, "using mock LLM content (SIM_MOCK_LLM=true)");
        return Ok((mock_content(rooms_needed), Usage::default()));
    }

    let messages = build_messages(config, rooms_needed);

    let request = ChatRequest {
        model: config.openrouter_model.clone(),
        messages,
        response_format: Some(ResponseFormat {
            r#type: "json_object".to_string(),
        }),
        temperature: Some(0.9),
        plugins: Vec::new(),
    };

    let response = client
        .post(format!("{}/chat/completions", config.llm_base_url))
        .header(
            "Authorization",
            format!("Bearer {}", config.openrouter_api_key),
        )
        .json(&request)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("OpenRouter API returned {status}: {body}"));
    }

    let chat_response: ChatResponse = response.json().await?;

    let usage = chat_response.usage.unwrap_or_default();
    tracing::info!(
        model = %config.openrouter_model,
        prompt_tokens = usage.prompt_tokens,
        completion_tokens = usage.completion_tokens,
        total_tokens = usage.total_tokens,
        "llm_call"
    );

    let first_choice = chat_response
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("OpenRouter returned empty choices array"))?;

    let content: SimContent = serde_json::from_str(&first_choice.message.content)?;

    Ok((content, usage))
}

// ---------------------------------------------------------------------------
// Brand ethics types
// ---------------------------------------------------------------------------

/// Response from Phase 1: company curation LLM call.
#[derive(Debug, Deserialize)]
pub struct CompanyCuration {
    pub companies: Vec<CuratedCompany>,
}

/// A single company selected by the curation LLM.
#[derive(Debug, Deserialize)]
pub struct CuratedCompany {
    pub ticker: String,
    pub name: String,
    pub relevance_hook: String,
}

/// Response from Phase 2: per-company evidence LLM call.
#[derive(Debug, Deserialize)]
pub struct CompanyEvidence {
    pub relevance_hook: String,
    pub dimensions: HashMap<String, DimensionEvidence>,
}

/// Pro/con evidence for a single ethical dimension.
#[derive(Debug, Deserialize)]
pub struct DimensionEvidence {
    pub pro: Vec<String>,
    pub con: Vec<String>,
}

// ---------------------------------------------------------------------------
// Brand ethics prompts
// ---------------------------------------------------------------------------

const BRAND_CURATION_SYSTEM: &str = r#"You are a research analyst selecting companies for an ethical evaluation platform. Your job is to identify S&P 500 companies that deeply affect people's daily lives but have low brand awareness. Deprioritize household tech and retail names (Apple, Amazon, Google, Walmart) — everyone already has opinions on those. Prioritize companies where users would say "I had no idea they were involved in that.""#;

const BRAND_EVIDENCE_SYSTEM: &str = r"You are a balanced research analyst providing factual context for ethical evaluation. For each dimension, provide 1-2 claims supporting the positive end and 1-2 claims supporting the negative end. Claims should be one sentence, factual in tone, and specific to the company. Include a source attribution if you can cite a specific report or organization; otherwise omit the source.";

fn build_brand_curation_messages(count: usize) -> Vec<ChatMessage> {
    let user_content = format!(
        r#"Identify exactly {count} S&P 500 companies ranked by surprising personal relevance — \
companies that deeply affect everyday life but that most people wouldn't recognize by name.

Respond with ONLY valid JSON matching this schema:
{{
  "companies": [
    {{
      "ticker": "SYY",
      "name": "Sysco Corporation",
      "relevance_hook": "Supplies food ingredients to nearly every restaurant and hospital cafeteria in the US."
    }}
  ]
}}"#
    );

    vec![
        ChatMessage {
            role: "system".to_string(),
            content: BRAND_CURATION_SYSTEM.to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: user_content,
        },
    ]
}

fn build_brand_evidence_messages(company_name: &str, ticker: &str) -> Vec<ChatMessage> {
    let user_content = format!(
        r#"Provide ethical evidence for {company_name} ({ticker}) across these five dimensions:

1. Labor Practices (Exploitative ↔ Exemplary)
2. Environmental Impact (Destructive ↔ Regenerative)
3. Consumer Trust (Deceptive ↔ Transparent)
4. Community Impact (Extractive ↔ Invested)
5. Corporate Governance (Self-Serving ↔ Accountable)

Respond with ONLY valid JSON matching this schema:
{{
  "relevance_hook": "One sentence explaining how this company affects daily life.",
  "dimensions": {{
    "Labor Practices": {{
      "pro": ["Positive claim about labor."],
      "con": ["Negative claim about labor."]
    }},
    "Environmental Impact": {{
      "pro": ["Positive environmental claim."],
      "con": ["Negative environmental claim."]
    }},
    "Consumer Trust": {{
      "pro": ["Positive consumer trust claim."],
      "con": ["Negative consumer trust claim."]
    }},
    "Community Impact": {{
      "pro": ["Positive community claim."],
      "con": ["Negative community claim."]
    }},
    "Corporate Governance": {{
      "pro": ["Positive governance claim."],
      "con": ["Negative governance claim."]
    }}
  }}
}}"#
    );

    vec![
        ChatMessage {
            role: "system".to_string(),
            content: BRAND_EVIDENCE_SYSTEM.to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: user_content,
        },
    ]
}

// ---------------------------------------------------------------------------
// Brand ethics mock data
// ---------------------------------------------------------------------------

const MOCK_COMPANIES: &[(&str, &str, &str)] = &[
    (
        "SYY",
        "Sysco Corporation",
        "Supplies food to schools and hospitals.",
    ),
    (
        "CARR",
        "Carrier Global",
        "Makes most commercial HVAC systems.",
    ),
    (
        "RSG",
        "Republic Services",
        "Handles trash and recycling for millions of households.",
    ),
    (
        "MKC",
        "McCormick & Company",
        "Supplies spices and flavorings to nearly every grocery brand.",
    ),
    (
        "CTAS",
        "Cintas Corporation",
        "Provides uniforms and safety gear to workplaces across the US.",
    ),
];

const MOCK_DIMENSION_NAMES: &[&str] = &[
    "Labor Practices",
    "Environmental Impact",
    "Consumer Trust",
    "Community Impact",
    "Corporate Governance",
];

fn mock_company_curation(count: usize) -> CompanyCuration {
    let companies = MOCK_COMPANIES
        .iter()
        .cycle()
        .take(count)
        .map(|(ticker, name, hook)| CuratedCompany {
            ticker: (*ticker).to_string(),
            name: (*name).to_string(),
            relevance_hook: (*hook).to_string(),
        })
        .collect();

    CompanyCuration { companies }
}

fn mock_company_evidence(company_name: &str) -> CompanyEvidence {
    let dimensions = MOCK_DIMENSION_NAMES
        .iter()
        .map(|dim| {
            (
                (*dim).to_string(),
                DimensionEvidence {
                    pro: vec!["Positive claim.".to_string()],
                    con: vec!["Negative claim.".to_string()],
                },
            )
        })
        .collect();

    CompanyEvidence {
        relevance_hook: format!(
            "{company_name} affects daily life in ways most people don't realize."
        ),
        dimensions,
    }
}

// ---------------------------------------------------------------------------
// Brand ethics generation functions
// ---------------------------------------------------------------------------

/// Phase 1: Ask LLM to curate companies from S&P 500.
///
/// Returns the curated company list and token usage for this call.
///
/// # Errors
///
/// Returns an error if the HTTP request fails, the response cannot be parsed,
/// or the LLM returns an empty choices array.
pub async fn generate_company_curation(
    client: &reqwest::Client,
    config: &SimConfig,
    count: usize,
) -> Result<(CompanyCuration, Usage), anyhow::Error> {
    if config.mock_llm {
        tracing::info!(count, "using mock company curation (SIM_MOCK_LLM=true)");
        return Ok((mock_company_curation(count), Usage::default()));
    }

    let messages = build_brand_curation_messages(count);

    let request = ChatRequest {
        model: config.evidence_model.clone(),
        messages,
        response_format: Some(ResponseFormat {
            r#type: "json_object".to_string(),
        }),
        temperature: Some(0.7),
        plugins: Vec::new(),
    };

    let response = client
        .post(format!("{}/chat/completions", config.llm_base_url))
        .header(
            "Authorization",
            format!("Bearer {}", config.openrouter_api_key),
        )
        .json(&request)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("OpenRouter API returned {status}: {body}"));
    }

    let chat_response: ChatResponse = response.json().await?;

    let usage = chat_response.usage.unwrap_or_default();
    tracing::info!(
        model = %config.evidence_model,
        prompt_tokens = usage.prompt_tokens,
        completion_tokens = usage.completion_tokens,
        total_tokens = usage.total_tokens,
        "llm_call brand_curation"
    );

    let first_choice = chat_response
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("OpenRouter returned empty choices array"))?;

    let json_str = extract_json(&first_choice.message.content);
    let curation: CompanyCuration = serde_json::from_str(json_str).map_err(|e| {
        anyhow::anyhow!(
            "failed to parse curation JSON: {e}\nraw: {}",
            first_choice.message.content
        )
    })?;

    Ok((curation, usage))
}

/// Phase 2: Generate evidence via Exa search + LLM synthesis.
///
/// Runs 5 parallel Exa searches (one per dimension), then synthesizes
/// pro/con evidence cards from the search results via a single LLM call.
///
/// # Errors
///
/// Returns an error if any Exa search or the synthesis LLM call fails.
#[allow(clippy::too_many_lines)]
pub async fn generate_company_evidence(
    client: &reqwest::Client,
    config: &SimConfig,
    company_name: &str,
    ticker: &str,
) -> Result<(CompanyEvidence, Usage), anyhow::Error> {
    if config.mock_llm {
        tracing::info!(
            company_name,
            ticker,
            "using mock company evidence (SIM_MOCK_LLM=true)"
        );
        return Ok((mock_company_evidence(company_name), Usage::default()));
    }

    // Step 1: Parallel Exa searches — one per dimension
    let dimensions = [
        "Labor Practices",
        "Environmental Impact",
        "Consumer Trust",
        "Community Impact",
        "Corporate Governance",
    ];

    // Spawn all 5 searches concurrently via JoinSet
    let mut join_set = tokio::task::JoinSet::new();
    for dim in &dimensions {
        let client = client.clone();
        let api_key = config.exa_api_key.clone();
        let base_url = config.exa_base_url.clone();
        let company = company_name.to_string();
        let dim_name = (*dim).to_string();
        join_set.spawn(async move {
            let result = exa_search(&client, &api_key, &base_url, &company, &dim_name).await;
            (dim_name, result)
        });
    }

    // Collect results as they complete
    let mut dim_results: HashMap<String, Vec<ExaResult>> = HashMap::new();
    while let Some(result) = join_set.join_next().await {
        if let Ok((dim_name, search_result)) = result {
            match search_result {
                Ok(results) => {
                    tracing::info!(
                        dimension = %dim_name,
                        results = results.len(),
                        company_name,
                        "exa_search complete"
                    );
                    dim_results.insert(dim_name, results);
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
    }

    // Build search context in dimension order
    let mut search_context = String::new();
    for dim in &dimensions {
        if let Some(results) = dim_results.get(*dim) {
            write!(search_context, "\n## {dim}\n\n").ok();
            for r in results {
                let title = r.title.as_deref().unwrap_or("(no title)");
                let text = r.text.as_deref().unwrap_or("");
                write!(
                    search_context,
                    "### [{title}]({url})\n{text}\n\n",
                    url = r.url
                )
                .ok();
            }
        }
    }

    if search_context.is_empty() {
        return Err(anyhow::anyhow!(
            "all Exa searches failed for {company_name} — cannot synthesize evidence"
        ));
    }

    // Step 2: Synthesize evidence from search results via LLM
    let messages = build_exa_synthesis_messages(company_name, ticker, &search_context);

    let request = ChatRequest {
        model: config.evidence_model.clone(),
        messages,
        response_format: Some(ResponseFormat {
            r#type: "json_object".to_string(),
        }),
        temperature: Some(0.3),
        plugins: Vec::new(),
    };

    let response = client
        .post(format!("{}/chat/completions", config.llm_base_url))
        .header(
            "Authorization",
            format!("Bearer {}", config.openrouter_api_key),
        )
        .json(&request)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "OpenRouter synthesis returned {status}: {body}"
        ));
    }

    let chat_response: ChatResponse = response.json().await?;

    let usage = chat_response.usage.unwrap_or_default();
    tracing::info!(
        model = %config.evidence_model,
        prompt_tokens = usage.prompt_tokens,
        completion_tokens = usage.completion_tokens,
        total_tokens = usage.total_tokens,
        company_name,
        ticker,
        "llm_call exa_synthesis"
    );

    let first_choice = chat_response
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("OpenRouter returned empty choices array"))?;

    let json_str = extract_json(&first_choice.message.content);
    let evidence: CompanyEvidence = serde_json::from_str(json_str).map_err(|e| {
        anyhow::anyhow!(
            "failed to parse synthesis JSON: {e}\nraw: {}",
            first_choice.message.content
        )
    })?;

    Ok((evidence, usage))
}

/// Search Exa for evidence about a company on a specific dimension.
async fn exa_search(
    client: &reqwest::Client,
    api_key: &str,
    exa_base_url: &str,
    company_name: &str,
    dimension: &str,
) -> Result<Vec<ExaResult>, anyhow::Error> {
    let request = ExaSearchRequest {
        query: format!("{company_name} {dimension}"),
        num_results: 5,
        r#type: "auto".to_string(),
        contents: ExaContentsOptions {
            text: ExaTextOptions {
                max_characters: 3000,
            },
        },
    };

    let response = client
        .post(format!("{exa_base_url}/search"))
        .header("x-api-key", api_key)
        .json(&request)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Exa API returned {status}: {body}"));
    }

    let exa_response: ExaSearchResponse = response.json().await?;
    Ok(exa_response.results)
}

const EXA_SYNTHESIS_SYSTEM: &str = r"You are a balanced research analyst extracting structured evidence from search results. For each of the 5 ethical dimensions, extract 2-3 specific, factual pro and con claims directly supported by the search results provided. Each claim must be one sentence and grounded in the sources — do not fabricate claims. If a dimension has weak search coverage, provide fewer claims rather than speculating.";

fn build_exa_synthesis_messages(
    company_name: &str,
    ticker: &str,
    search_context: &str,
) -> Vec<ChatMessage> {
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
        ChatMessage {
            role: "system".to_string(),
            content: EXA_SYNTHESIS_SYSTEM.to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: user_content,
        },
    ]
}

/// Query `OpenRouter`'s generation stats endpoint for cost data.
///
/// # Errors
///
/// Returns an error if the HTTP request fails.
pub async fn get_generation_cost(
    client: &reqwest::Client,
    api_key: &str,
    llm_base_url: &str,
    generation_id: &str,
) -> Result<Option<f64>, anyhow::Error> {
    let url = format!("{llm_base_url}/generation?id={generation_id}");

    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .send()
        .await?;

    if !resp.status().is_success() {
        tracing::warn!(
            status = %resp.status(),
            "failed to fetch generation stats (cost will be unavailable)"
        );
        return Ok(None);
    }

    let stats: GenerationStats = resp.json().await?;
    Ok(stats.total_cost)
}

/// Generate company evidence with explicit model and search overrides.
/// Used by the battery test to compare outputs across model/search combinations.
///
/// Returns `(evidence, usage, raw_response, generation_id)`.
///
/// # Errors
///
/// Returns an error if the HTTP request fails or the response cannot be parsed.
pub async fn generate_company_evidence_with_overrides(
    client: &reqwest::Client,
    api_key: &str,
    llm_base_url: &str,
    model: &str,
    search: bool,
    company_name: &str,
    ticker: &str,
) -> Result<(CompanyEvidence, Usage, String, String), anyhow::Error> {
    let messages = build_brand_evidence_messages(company_name, ticker);

    let plugins = if search {
        vec![Plugin {
            id: "web".to_string(),
        }]
    } else {
        Vec::new()
    };

    // Deep research and Perplexity models don't support json_object format;
    // reasoning models (o3/o4) don't support temperature.
    let is_deep_research = model.contains("deep-research");
    let is_perplexity = model.starts_with("perplexity/");
    let is_reasoning = model.contains("/o3") || model.contains("/o4");

    let response_format = if is_deep_research || is_perplexity {
        None
    } else {
        Some(ResponseFormat {
            r#type: "json_object".to_string(),
        })
    };

    let temperature = if is_deep_research || is_reasoning {
        None
    } else {
        Some(0.7)
    };

    let request = ChatRequest {
        model: model.to_string(),
        messages,
        response_format,
        temperature,
        plugins,
    };

    let response = client
        .post(format!("{llm_base_url}/chat/completions"))
        .header("Authorization", format!("Bearer {api_key}"))
        .json(&request)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("OpenRouter API returned {status}: {body}"));
    }

    let chat_response: ChatResponse = response.json().await?;

    let generation_id = chat_response.id.clone();
    let usage = chat_response.usage.unwrap_or_default();
    tracing::info!(
        model,
        search,
        prompt_tokens = usage.prompt_tokens,
        completion_tokens = usage.completion_tokens,
        total_tokens = usage.total_tokens,
        generation_id = %generation_id,
        company_name,
        "llm_call battery"
    );

    let first_choice = chat_response
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("OpenRouter returned empty choices array"))?;

    // Return raw content alongside parsed evidence for comparison.
    // Deep research / Perplexity models may wrap JSON in markdown fencing
    // or return prose with embedded JSON — try to extract it.
    let raw = first_choice.message.content;
    let json_str = extract_json(&raw);
    let evidence: CompanyEvidence = serde_json::from_str(json_str)
        .map_err(|e| anyhow::anyhow!("failed to parse evidence JSON: {e}\nraw: {raw}"))?;

    Ok((evidence, usage, raw, generation_id))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract JSON from a response that may be wrapped in markdown fencing
/// (e.g., `` ```json ... ``` ``) or contain prose around the JSON object.
/// Returns the input unchanged if it already starts with `{`.
fn extract_json(raw: &str) -> &str {
    let trimmed = raw.trim();

    // Already valid JSON start
    if trimmed.starts_with('{') {
        return trimmed;
    }

    // Strip markdown fencing: ```json ... ``` or ``` ... ```
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if end > start {
                return &trimmed[start..=end];
            }
        }
    }

    trimmed
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_valid_llm_response() {
        let json = r#"{
            "rooms": [
                {
                    "name": "Downtown Revitalization",
                    "description": "Proposals for improving the downtown area",
                    "polls": [
                        {
                            "question": "Should we add more bike lanes downtown?",
                            "description": "Evaluating cycling infrastructure expansion",
                            "dimensions": [
                                {
                                    "name": "Safety Impact",
                                    "description": "Expected improvement to cyclist safety",
                                    "min": 0.0,
                                    "max": 10.0,
                                    "min_label": "No improvement",
                                    "max_label": "Major improvement"
                                },
                                {
                                    "name": "Cost Effectiveness",
                                    "description": "Value relative to projected costs",
                                    "min": 0.0,
                                    "max": 5.0,
                                    "min_label": "Poor value",
                                    "max_label": "Excellent value"
                                }
                            ]
                        }
                    ]
                }
            ]
        }"#;

        let content: SimContent = serde_json::from_str(json).expect("valid JSON");
        assert_eq!(content.rooms.len(), 1);
        assert_eq!(content.rooms[0].name, "Downtown Revitalization");
        assert_eq!(content.rooms[0].polls.len(), 1);
        assert_eq!(content.rooms[0].polls[0].dimensions.len(), 2);
        assert!((content.rooms[0].polls[0].dimensions[1].max - 5.0).abs() < f32::EPSILON);
    }

    #[test]
    fn deserializes_empty_rooms_array() {
        let json = r#"{"rooms": []}"#;
        let content: SimContent = serde_json::from_str(json).expect("valid JSON");
        assert!(content.rooms.is_empty());
    }

    #[test]
    fn mock_content_generates_requested_room_count() {
        let content = mock_content(3);
        assert_eq!(content.rooms.len(), 3);
        for (i, room) in content.rooms.iter().enumerate() {
            assert_eq!(room.name, format!("Mock Room {}", i + 1));
            assert_eq!(room.polls.len(), 2, "each room should have 2 polls");
            // First poll has 2 dimensions, second has 3
            assert_eq!(room.polls[0].dimensions.len(), 2);
            assert_eq!(room.polls[1].dimensions.len(), 3);
            // All dimensions should have labels
            for poll in &room.polls {
                for dim in &poll.dimensions {
                    assert!(dim.min_label.is_some(), "dimension should have min_label");
                    assert!(dim.max_label.is_some(), "dimension should have max_label");
                    assert!(dim.max > dim.min, "max should be greater than min");
                }
            }
        }
    }

    #[test]
    fn mock_content_zero_rooms() {
        let content = mock_content(0);
        assert!(content.rooms.is_empty());
    }

    #[tokio::test]
    async fn generate_content_returns_mock_when_enabled() {
        let config = SimConfig {
            api_url: "http://localhost:4000".to_string(),
            openrouter_api_key: String::new(),
            openrouter_model: "unused".to_string(),
            target_rooms: 5,
            votes_per_poll: 15,
            system_prompt: "unused".to_string(),
            voter_count: 20,
            log_level: "info".to_string(),
            mock_llm: true,
            poll_duration_secs: 86400,
            room_topic: "civic".to_string(),
            company_count: 25,
            dry_run: false,
            battery_config: None,
            battery_company: None,
            battery_ticker: None,
            exa_api_key: String::new(),
            evidence_model: "deepseek/deepseek-v3.2".to_string(),
            llm_base_url: "https://openrouter.ai/api/v1".to_string(),
            exa_base_url: "https://api.exa.ai".to_string(),
        };

        let client = reqwest::Client::new();
        let (content, usage) = generate_content(&client, &config, 2).await.unwrap();

        assert_eq!(content.rooms.len(), 2);
        assert_eq!(usage.total_tokens, 0, "mock should report zero tokens");
    }

    #[test]
    fn builds_correct_messages() {
        let config = SimConfig {
            api_url: "http://localhost:4000".to_string(),
            openrouter_api_key: "test-key".to_string(),
            openrouter_model: "test-model".to_string(),
            target_rooms: 5,
            votes_per_poll: 15,
            system_prompt: "You are a test system.".to_string(),
            voter_count: 20,
            log_level: "info".to_string(),
            mock_llm: false,
            poll_duration_secs: 86400,
            room_topic: "civic".to_string(),
            company_count: 25,
            dry_run: false,
            battery_config: None,
            battery_company: None,
            battery_ticker: None,
            exa_api_key: String::new(),
            evidence_model: "deepseek/deepseek-v3.2".to_string(),
            llm_base_url: "https://openrouter.ai/api/v1".to_string(),
            exa_base_url: "https://api.exa.ai".to_string(),
        };

        let messages = build_messages(&config, 2);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[0].content, "You are a test system.");
        assert_eq!(messages[1].role, "user");
        assert!(
            messages[1].content.contains('2'),
            "user message should contain the room count"
        );
    }
}

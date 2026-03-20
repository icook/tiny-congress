//! Shared LLM chat completion and Exa search client with cache header detection.
//!
//! This crate provides generic functions for calling `OpenRouter`-compatible LLM APIs
//! and the Exa search API, with built-in detection of cache hits across three layers:
//! `OpenRouter` prompt cache, `LiteLLM` proxy cache, and nginx reverse proxy cache.

use std::ops::AddAssign;

use anyhow::Context as _;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Breakdown of prompt tokens, including cache hits.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct PromptTokenDetails {
    #[serde(default)]
    pub cached_tokens: u32,
}

/// Token usage from a single `OpenRouter` API call.
#[derive(Debug, Default, Clone, Copy, Deserialize, Serialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    /// Cost in USD for this API call, if returned inline by `OpenRouter`.
    #[serde(default)]
    pub cost: Option<f64>,
    /// Prompt token breakdown, including cache hit counts.
    #[serde(default)]
    pub prompt_tokens_details: Option<PromptTokenDetails>,
}

/// Note: `prompt_tokens_details` is **not** accumulated — it reflects only the
/// last assignment.  This field is per-request metadata, not a running total.
impl AddAssign for Usage {
    fn add_assign(&mut self, rhs: Self) {
        self.prompt_tokens += rhs.prompt_tokens;
        self.completion_tokens += rhs.completion_tokens;
        self.total_tokens += rhs.total_tokens;
        match (self.cost, rhs.cost) {
            (Some(a), Some(b)) => self.cost = Some(a + b),
            (None, Some(b)) => self.cost = Some(b),
            _ => {}
        }
    }
}

/// A single chat message in an OpenRouter-compatible request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Cache detection results from HTTP response headers and body.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CacheInfo {
    /// `OpenRouter` prompt cache: tokens read from cache.
    pub openrouter_cached_tokens: Option<u32>,
    /// `LiteLLM` proxy: full response served from cache.
    pub litellm_hit: bool,
    /// Nginx: response served from cache (for Exa).
    pub nginx_hit: bool,
}

/// Result of a single chat completion call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletion {
    pub content: String,
    pub usage: Usage,
    pub cache: CacheInfo,
    pub generation_id: Option<String>,
    pub model: String,
}

/// A single search result from Exa.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub url: String,
    pub title: String,
    pub text: String,
}

/// Result of an Exa search call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub cache: CacheInfo,
}

// ---------------------------------------------------------------------------
// Private API types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
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

#[derive(Debug, Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChoiceMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct GenerationStats {
    total_cost: Option<f64>,
}

// ---------------------------------------------------------------------------
// Exa API private types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExaSearchRequest {
    query: String,
    num_results: usize,
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
    results: Vec<ExaResultRaw>,
}

#[derive(Debug, Deserialize)]
struct ExaResultRaw {
    url: String,
    title: Option<String>,
    text: Option<String>,
}

// ---------------------------------------------------------------------------
// Public functions
// ---------------------------------------------------------------------------

/// Call an OpenRouter-compatible LLM API for chat completion.
///
/// Detects cache hits from response headers before consuming the body:
/// - `x-litellm-cache-hit: True` → `cache.litellm_hit = true`
/// - `x-cache` containing `"HIT"` → `cache.litellm_hit = true` (fallback)
/// - `usage.prompt_tokens_details.cached_tokens` → `cache.openrouter_cached_tokens`
///
/// # Errors
///
/// Returns an error if the HTTP request fails, the API returns a non-success
/// status, the response body cannot be parsed, or the choices array is empty.
pub async fn chat_completion(
    client: &reqwest::Client,
    api_key: &str,
    base_url: &str,
    model: &str,
    messages: Vec<ChatMessage>,
    json_mode: bool,
    temperature: Option<f32>,
) -> Result<ChatCompletion, anyhow::Error> {
    let request = ChatRequest {
        model: model.to_string(),
        messages,
        response_format: if json_mode {
            Some(ResponseFormat {
                r#type: "json_object".to_string(),
            })
        } else {
            None
        },
        temperature,
    };

    let response = client
        .post(format!("{base_url}/chat/completions"))
        .header("Authorization", format!("Bearer {api_key}"))
        .json(&request)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("LLM API returned {status}: {body}"));
    }

    // Read cache headers before consuming the body.
    //
    // LiteLLM signals cache hits in multiple ways across versions:
    // - `x-litellm-cache-hit: True` (older versions)
    // - `x-litellm-cache-key: <hash>` (v1.82+, present only on cache hits)
    // - `x-cache: HIT` (some proxy configs)
    let litellm_hit = {
        let explicit_hit = response
            .headers()
            .get("x-litellm-cache-hit")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .eq_ignore_ascii_case("true");
        let has_cache_key = response.headers().contains_key("x-litellm-cache-key");
        let x_cache_hit = response
            .headers()
            .get("x-cache")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_uppercase()
            .contains("HIT");
        explicit_hit || has_cache_key || x_cache_hit
    };

    let chat_response: ChatResponse = response
        .json()
        .await
        .context("parsing LLM chat completion response")?;

    let usage = chat_response.usage.unwrap_or_default();
    let openrouter_cached_tokens = usage
        .prompt_tokens_details
        .map(|d| d.cached_tokens)
        .filter(|&t| t > 0);

    tracing::info!(
        model,
        prompt_tokens = usage.prompt_tokens,
        completion_tokens = usage.completion_tokens,
        total_tokens = usage.total_tokens,
        "llm_call"
    );

    let first_choice = chat_response
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("LLM API returned empty choices array"))?;

    Ok(ChatCompletion {
        content: first_choice.message.content,
        usage,
        cache: CacheInfo {
            openrouter_cached_tokens,
            litellm_hit,
            nginx_hit: false,
        },
        generation_id: Some(chat_response.id),
        model: model.to_string(),
    })
}

/// Search Exa for web content matching a query.
///
/// Detects nginx cache hits from response headers before consuming the body:
/// - `x-cache` or `x-cache-status` containing `"HIT"` → `cache.nginx_hit = true`
///
/// Uses `x-api-key` header for authentication (not `Authorization: Bearer`).
///
/// # Errors
///
/// Returns an error if the HTTP request fails, the API returns a non-success
/// status, or the response body cannot be parsed.
pub async fn exa_search(
    client: &reqwest::Client,
    api_key: &str,
    base_url: &str,
    query: &str,
    num_results: usize,
) -> Result<SearchResponse, anyhow::Error> {
    let request = ExaSearchRequest {
        query: query.to_string(),
        num_results,
        r#type: "auto".to_string(),
        contents: ExaContentsOptions {
            text: ExaTextOptions {
                max_characters: 3000,
            },
        },
    };

    let response = client
        .post(format!("{base_url}/search"))
        .header("x-api-key", api_key)
        .json(&request)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Exa API returned {status}: {body}"));
    }

    // Read cache headers before consuming the body
    let nginx_hit = {
        let x_cache = response
            .headers()
            .get("x-cache")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let x_cache_status = response
            .headers()
            .get("x-cache-status")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        x_cache.to_ascii_uppercase().contains("HIT")
            || x_cache_status.to_ascii_uppercase().contains("HIT")
    };

    let exa_response: ExaSearchResponse = response
        .json()
        .await
        .context("parsing Exa search response")?;

    let results = exa_response
        .results
        .into_iter()
        .map(|r| SearchResult {
            url: r.url,
            title: r.title.unwrap_or_default(),
            text: r.text.unwrap_or_default(),
        })
        .collect();

    Ok(SearchResponse {
        results,
        cache: CacheInfo {
            openrouter_cached_tokens: None,
            litellm_hit: false,
            nginx_hit,
        },
    })
}

/// Extract JSON from a response that may be wrapped in markdown fencing
/// (e.g., `` ```json ... ``` ``) or contain prose around the JSON object.
/// Returns the trimmed input if it already starts with `{`.
#[must_use]
pub fn extract_json(raw: &str) -> &str {
    let trimmed = raw.trim();

    if trimmed.starts_with('{') {
        return trimmed;
    }

    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if end > start {
                return &trimmed[start..=end];
            }
        }
    }

    trimmed
}

/// Query `OpenRouter`'s generation stats endpoint for cost data.
///
/// # Errors
///
/// Returns an error if the HTTP request fails.
pub async fn get_generation_cost(
    client: &reqwest::Client,
    api_key: &str,
    base_url: &str,
    generation_id: &str,
) -> Result<Option<f64>, anyhow::Error> {
    let url = format!("{base_url}/generation?id={generation_id}");

    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .send()
        .await?;

    if !resp.status().is_success() {
        tracing::warn!(status = %resp.status(), "failed to fetch generation stats");
        return Ok(None);
    }

    let stats: GenerationStats = resp
        .json()
        .await
        .context("parsing generation stats response")?;
    Ok(stats.total_cost)
}

// ---------------------------------------------------------------------------
// Shared research pipeline constants and types
// ---------------------------------------------------------------------------

/// Fixed ethical dimensions for brand ethics research.
/// Each tuple: (name, `min_label`, `max_label`).
pub const DIMENSIONS: &[(&str, &str, &str)] = &[
    ("Labor Practices", "Exploitative", "Exemplary"),
    ("Environmental Impact", "Destructive", "Regenerative"),
    ("Consumer Trust", "Deceptive", "Transparent"),
    ("Community Impact", "Extractive", "Invested"),
    ("Corporate Governance", "Self-Serving", "Accountable"),
];

/// Default system prompt for evidence synthesis.
pub const DEFAULT_SYNTHESIS_SYSTEM_PROMPT: &str = r"You are a balanced research analyst extracting structured evidence from search results. For each of the 5 ethical dimensions, extract 2-3 specific, factual pro and con claims directly supported by the search results provided. Each claim must be one sentence and grounded in the sources — do not fabricate claims. If a dimension has weak search coverage, provide fewer claims rather than speculating.";

/// Top-level LLM response for company evidence synthesis.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CompanyEvidence {
    pub relevance_hook: String,
    pub dimensions: std::collections::HashMap<String, DimensionEvidence>,
}

/// Pro/con evidence for a single ethical dimension.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DimensionEvidence {
    pub pro: Vec<String>,
    pub con: Vec<String>,
}

/// Build the synthesis messages for the LLM.
///
/// `system_prompt` overrides the default synthesis prompt when `Some`.
#[must_use]
pub fn build_synthesis_messages(
    company_name: &str,
    ticker: &str,
    search_context: &str,
    system_prompt: Option<&str>,
) -> Vec<ChatMessage> {
    let system = system_prompt.unwrap_or(DEFAULT_SYNTHESIS_SYSTEM_PROMPT);
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
            content: system.to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: user_content,
        },
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_passthrough_for_bare_object() {
        let raw = r#"{"key": "value"}"#;
        assert_eq!(extract_json(raw), raw);
    }

    #[test]
    fn extract_json_strips_markdown_fencing() {
        let raw = "```json\n{\"key\": \"value\"}\n```";
        assert_eq!(extract_json(raw), r#"{"key": "value"}"#);
    }

    #[test]
    fn extract_json_strips_prose_prefix() {
        let raw = "Here is the JSON: {\"key\": \"value\"}";
        assert_eq!(extract_json(raw), r#"{"key": "value"}"#);
    }

    #[test]
    fn extract_json_returns_trimmed_when_no_braces() {
        let raw = "  no json here  ";
        assert_eq!(extract_json(raw), "no json here");
    }

    #[test]
    fn usage_add_assign_accumulates_tokens() {
        let mut a = Usage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
            cost: Some(0.001),
            prompt_tokens_details: None,
        };
        let b = Usage {
            prompt_tokens: 20,
            completion_tokens: 10,
            total_tokens: 30,
            cost: Some(0.002),
            prompt_tokens_details: None,
        };
        a += b;
        assert_eq!(a.prompt_tokens, 30);
        assert_eq!(a.completion_tokens, 15);
        assert_eq!(a.total_tokens, 45);
        assert!((a.cost.unwrap() - 0.003).abs() < f64::EPSILON);
    }

    #[test]
    fn usage_add_assign_handles_none_cost() {
        let mut a = Usage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
            cost: None,
            prompt_tokens_details: None,
        };
        let b = Usage {
            prompt_tokens: 20,
            completion_tokens: 10,
            total_tokens: 30,
            cost: Some(0.002),
            prompt_tokens_details: None,
        };
        a += b;
        assert!((a.cost.unwrap() - 0.002).abs() < f64::EPSILON);
    }
}

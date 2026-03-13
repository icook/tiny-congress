//! LLM response types and `OpenRouter` client for generating sim content.

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
#[derive(Debug, Default, Clone, Copy, Deserialize)]
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
    response_format: ResponseFormat,
    temperature: f32,
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
        response_format: ResponseFormat {
            r#type: "json_object".to_string(),
        },
        temperature: 0.9,
    };

    let response = client
        .post("https://openrouter.ai/api/v1/chat/completions")
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

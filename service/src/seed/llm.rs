//! LLM response types and `OpenRouter` client for generating seed content.

use serde::{Deserialize, Serialize};

use super::config::SeedConfig;

// ---------------------------------------------------------------------------
// Public response types – deserialized from LLM JSON output
// ---------------------------------------------------------------------------

/// Top-level LLM response containing generated rooms.
#[derive(Debug, Clone, Deserialize)]
pub struct SeedContent {
    pub rooms: Vec<SeedRoom>,
}

/// A generated room with its polls.
#[derive(Debug, Clone, Deserialize)]
pub struct SeedRoom {
    pub name: String,
    pub description: String,
    pub polls: Vec<SeedPoll>,
}

/// A generated poll with its dimensions.
#[derive(Debug, Clone, Deserialize)]
pub struct SeedPoll {
    pub question: String,
    pub description: String,
    pub dimensions: Vec<SeedDimension>,
}

/// A single dimension for opinion-space voting.
#[derive(Debug, Clone, Deserialize)]
pub struct SeedDimension {
    pub name: String,
    pub description: String,
    pub min: f32,
    pub max: f32,
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
              "max": 10.0
            }
          ]
        }
      ]
    }
  ]
}"#;

/// Build the system and user messages for the LLM request.
#[must_use]
pub fn build_messages(config: &SeedConfig, rooms_needed: usize) -> Vec<ChatMessage> {
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

/// Call the `OpenRouter` API to generate seed content.
///
/// # Errors
///
/// Returns an error if the HTTP request fails, the response cannot be parsed,
/// or the LLM returns an empty choices array.
pub async fn generate_content(
    client: &reqwest::Client,
    config: &SeedConfig,
    rooms_needed: usize,
) -> Result<SeedContent, anyhow::Error> {
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

    let first_choice = chat_response
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("OpenRouter returned empty choices array"))?;

    let content: SeedContent = serde_json::from_str(&first_choice.message.content)?;

    Ok(content)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
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
                                    "max": 10.0
                                },
                                {
                                    "name": "Cost Effectiveness",
                                    "description": "Value relative to projected costs",
                                    "min": 0.0,
                                    "max": 5.0
                                }
                            ]
                        }
                    ]
                }
            ]
        }"#;

        let content: SeedContent = serde_json::from_str(json).expect("valid JSON");
        assert_eq!(content.rooms.len(), 1);
        assert_eq!(content.rooms[0].name, "Downtown Revitalization");
        assert_eq!(content.rooms[0].polls.len(), 1);
        assert_eq!(content.rooms[0].polls[0].dimensions.len(), 2);
        assert!((content.rooms[0].polls[0].dimensions[1].max - 5.0).abs() < f32::EPSILON);
    }

    #[test]
    fn deserializes_empty_rooms_array() {
        let json = r#"{"rooms": []}"#;
        let content: SeedContent = serde_json::from_str(json).expect("valid JSON");
        assert!(content.rooms.is_empty());
    }

    #[test]
    fn builds_correct_messages() {
        let config = SeedConfig {
            openrouter_api_key: "test-key".to_string(),
            openrouter_model: "test-model".to_string(),
            target_rooms: 5,
            votes_per_poll: 15,
            system_prompt: "You are a test system.".to_string(),
            voter_count: 20,
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

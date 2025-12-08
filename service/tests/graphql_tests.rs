use async_graphql::{EmptySubscription, Schema};
use serde_json::Value;
use tinycongress_api::graphql::{MutationRoot, QueryRoot};

async fn execute_query(query: &str) -> Value {
    let schema = Schema::build(QueryRoot, MutationRoot, EmptySubscription).finish();
    let response = schema.execute(query).await;
    serde_json::to_value(response).unwrap()
}

#[tokio::test]
async fn test_current_round_query() {
    let query = r#"
        {
            currentRound {
                id
                status
            }
        }
    "#;

    let result = execute_query(query).await;

    // Get the data field
    let data = &result["data"];
    assert!(data.is_object());

    // Get the currentRound field
    let current_round = &data["currentRound"];
    assert!(current_round.is_object());

    // Check fields
    assert!(current_round["id"].is_string());
    assert_eq!(current_round["status"].as_str().unwrap(), "active");
}

#[tokio::test]
async fn test_top_topics_query() {
    let query = r#"
        {
            topTopics(limit: 3) {
                rank
                score
                topic {
                    id
                    title
                }
            }
        }
    "#;

    let result = execute_query(query).await;

    // Get the data field
    let data = &result["data"];
    assert!(data.is_object());

    // Get the topTopics field
    let top_topics = &data["topTopics"];
    assert!(top_topics.is_array());

    // Check array length
    let top_topics_array = top_topics.as_array().unwrap();
    assert_eq!(top_topics_array.len(), 3);

    // Check that topics are in descending order by score
    let scores: Vec<f64> = top_topics_array
        .iter()
        .map(|topic| topic["score"].as_f64().unwrap())
        .collect();

    for i in 0..scores.len() - 1 {
        assert!(scores[i] > scores[i + 1]);
    }
}

#[tokio::test]
async fn test_submit_vote_mutation() {
    let mutation = r#"
        mutation {
            submitVote(
                pairingId: "pairing-123",
                userId: "user-456",
                choice: "topic-1"
            )
        }
    "#;

    let result = execute_query(mutation).await;

    // Get the data field
    let data = &result["data"];
    assert!(data.is_object());

    // Check submit_vote
    let submit_vote = &data["submitVote"];
    assert!(submit_vote.is_boolean());
    assert!(submit_vote.as_bool().unwrap());
}

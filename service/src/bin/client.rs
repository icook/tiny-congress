#![deny(
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::unwrap_used
)]
#![allow(clippy::print_stdout)]

use std::time::Duration;
use tokio::time;
use uuid::Uuid;

#[tokio::main]
#[allow(clippy::too_many_lines)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // This is a simple client to demonstrate subscribing to round updates
    // and interacting with the GraphQL API
    println!("Starting prioritization room client");

    let client = reqwest::Client::new();
    let base_url = "http://localhost:3000/graphql";

    // Generate a random user ID for this client
    let user_id = Uuid::new_v4();
    println!("Client user ID: {user_id}");

    // Main client loop
    loop {
        // Query for the current round
        let query = r"
            query {
                currentRound {
                    id
                    startTime
                    endTime
                    status
                }
            }
        ";

        let resp = client
            .post(base_url)
            .json(&serde_json::json!({
                "query": query
            }))
            .send()
            .await?;

        let json = resp.json::<serde_json::Value>().await?;

        if let Some(round) = json["data"]["currentRound"].as_object() {
            let round_id = round["id"].as_str().unwrap_or_default();
            println!("Current round: {round_id}");

            if round_id.is_empty() {
                println!("No active round found");
            } else {
                let pairing_query = format!(
                    r#"
                    query {{
                        currentPairing(roundId: "{round_id}")
                        {{
                            id
                            topicA {{
                                id
                                title
                                description
                            }}
                            topicB {{
                                id
                                title
                                description
                            }}
                        }}
                    }}
                "#
                );

                let pairing_resp = client
                    .post(base_url)
                    .json(&serde_json::json!({
                        "query": pairing_query
                    }))
                    .send()
                    .await?;

                let pairing_json = pairing_resp.json::<serde_json::Value>().await?;

                if let Some(pairing) = pairing_json["data"]["currentPairing"].as_object() {
                    let pairing_id = pairing["id"].as_str().unwrap_or_default();
                    let topic_a = &pairing["topicA"];
                    let topic_b = &pairing["topicB"];

                    println!("\nCurrent Pairing: {pairing_id}");
                    println!(
                        "A: {} - {}",
                        topic_a["title"].as_str().unwrap_or_default(),
                        topic_a["description"].as_str().unwrap_or_default()
                    );
                    println!(
                        "B: {} - {}",
                        topic_b["title"].as_str().unwrap_or_default(),
                        topic_b["description"].as_str().unwrap_or_default()
                    );

                    // Simulate a vote (randomly choose A or B)
                    let choice = if rand::random::<bool>() {
                        topic_a["id"].as_str().unwrap_or_default()
                    } else {
                        topic_b["id"].as_str().unwrap_or_default()
                    };

                    println!("Voting for: {choice}");

                    let vote_mutation = format!(
                        r#"
                        mutation {{
                            submitVote(
                                pairingId: "{pairing_id}"
                                userId: "{user_id}"
                                choice: "{choice}"
                            )
                        }}
                    "#
                    );

                    let vote_resp = client
                        .post(base_url)
                        .json(&serde_json::json!({
                            "query": vote_mutation
                        }))
                        .send()
                        .await?;

                    let vote_json = vote_resp.json::<serde_json::Value>().await?;
                    println!("Vote submitted: {:?}", vote_json["data"]["submitVote"]);
                } else {
                    println!("No current pairing found");
                }
            }
        }

        // Query top topics
        let top_topics_query = r"
            query {
                topTopics(limit: 5) {
                    rank
                    score
                    topic {
                        title
                    }
                }
            }
        ";

        let topics_resp = client
            .post(base_url)
            .json(&serde_json::json!({
                "query": top_topics_query
            }))
            .send()
            .await?;

        let topics_json = topics_resp.json::<serde_json::Value>().await?;

        println!("\nTop 5 Topics:");
        if let Some(topics) = topics_json["data"]["topTopics"].as_array() {
            for topic in topics {
                println!(
                    "#{}: {} (Score: {})",
                    topic["rank"].as_i64().unwrap_or_default(),
                    topic["topic"]["title"].as_str().unwrap_or_default(),
                    topic["score"].as_f64().unwrap_or_default()
                );
            }
        }

        // Wait before next loop
        time::sleep(Duration::from_secs(10)).await;
    }
}

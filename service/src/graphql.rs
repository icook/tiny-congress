use async_graphql::http::{playground_source, GraphQLPlaygroundConfig};
use async_graphql::{Context, EmptySubscription, Object, Schema, SimpleObject, ID};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::extract::Extension;
use axum::response::{Html, IntoResponse};
use chrono::Utc;
use tracing::info;

// Define the schema type with Query and Mutation roots
pub type ApiSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

// Round model
#[derive(SimpleObject)]
pub struct Round {
    pub id: ID,
    pub start_time: String,
    pub end_time: String,
    pub status: String,
}

// Topic model
#[derive(SimpleObject)]
pub struct Topic {
    pub id: ID,
    pub title: String,
    pub description: String,
}

// Pairing model
#[derive(SimpleObject)]
pub struct Pairing {
    pub id: ID,
    pub topic_a: Topic,
    pub topic_b: Topic,
}

// TopicRanking model
#[derive(SimpleObject)]
pub struct TopicRanking {
    pub topic_id: ID,
    pub rank: i32,
    pub score: f64,
    pub topic: Topic,
}

// Query root
pub struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn current_round(&self, _ctx: &Context<'_>) -> Option<Round> {
        // Mock implementation
        Some(Round {
            id: ID::from("round-123"),
            start_time: Utc::now().to_rfc3339(),
            end_time: (Utc::now() + chrono::Duration::seconds(60)).to_rfc3339(),
            status: "active".to_string(),
        })
    }

    async fn current_pairing(&self, _ctx: &Context<'_>, round_id: ID) -> Option<Pairing> {
        // Mock implementation
        let pairing_id = ID::from(format!("pairing-{}", round_id.as_str()));
        Some(Pairing {
            id: pairing_id,
            topic_a: Topic {
                id: ID::from("topic-1"),
                title: "Climate Change".to_string(),
                description: "Address global climate change and its impacts".to_string(),
            },
            topic_b: Topic {
                id: ID::from("topic-2"),
                title: "Healthcare Reform".to_string(),
                description: "Improve healthcare access and affordability".to_string(),
            },
        })
    }

    async fn top_topics(&self, _ctx: &Context<'_>, limit: Option<i32>) -> Vec<TopicRanking> {
        // Mock implementation
        let limit = limit.unwrap_or(5);
        let mut rankings = Vec::new();

        for i in 1..=limit {
            rankings.push(TopicRanking {
                topic_id: ID::from(format!("topic-{i}")),
                rank: i,
                score: f64::from(i).mul_add(-50.0, 1500.0),
                topic: Topic {
                    id: ID::from(format!("topic-{i}")),
                    title: format!("Issue #{i}"),
                    description: format!("Description for issue #{i}"),
                },
            });
        }

        rankings
    }
}

// Mutation root
pub struct MutationRoot;

#[Object]
impl MutationRoot {
    async fn submit_vote(
        &self,
        _ctx: &Context<'_>,
        pairing_id: ID,
        user_id: ID,
        choice: ID,
    ) -> bool {
        // Mock implementation
        info!(
            user_id = %user_id.as_str(),
            choice = %choice.as_str(),
            pairing_id = %pairing_id.as_str(),
            "vote received"
        );
        true
    }
}

// GraphQL playground handler
#[allow(clippy::unused_async)]
pub async fn graphql_playground() -> impl IntoResponse {
    Html(playground_source(GraphQLPlaygroundConfig::new("/graphql")))
}

pub async fn graphql_handler(schema: Extension<ApiSchema>, req: GraphQLRequest) -> GraphQLResponse {
    schema.execute(req.into_inner()).await.into()
}

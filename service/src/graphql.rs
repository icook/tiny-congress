use crate::auth::OAuthService;
use async_graphql::http::{playground_source, GraphQLPlaygroundConfig};
use async_graphql::{Context, EmptySubscription, Enum, Object, Result, Schema, SimpleObject, ID};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::extract::Extension;
use axum::response::{Html, IntoResponse};
use chrono::Utc;
use oauth2::{AuthorizationCode, TokenResponse};
use tracing::error;

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

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum OAuthProvider {
    Google,
}

#[derive(SimpleObject)]
pub struct StartOAuthPayload {
    pub auth_url: String,
}

#[derive(SimpleObject)]
pub struct CompleteOAuthPayload {
    pub access_token: String,
    pub email: String,
    pub email_verified: bool,
    pub expires_at: Option<String>,
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

    async fn current_pairing(&self, _ctx: &Context<'_>, _round_id: ID) -> Option<Pairing> {
        // Mock implementation
        Some(Pairing {
            id: ID::from("pairing-123"),
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
                topic_id: ID::from(format!("topic-{}", i)),
                rank: i,
                score: 1500.0 - (i as f64 * 50.0),
                topic: Topic {
                    id: ID::from(format!("topic-{}", i)),
                    title: format!("Issue #{}", i),
                    description: format!("Description for issue #{}", i),
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
        println!(
            "Vote received: user {:?} voted for {:?} in pairing {:?}",
            user_id, choice, pairing_id
        );
        true
    }

    async fn start_oauth(
        &self,
        ctx: &Context<'_>,
        provider: OAuthProvider,
    ) -> Result<StartOAuthPayload> {
        let oauth_service = ctx.data::<OAuthService>()?;

        let google = match provider {
            OAuthProvider::Google => oauth_service
                .google
                .as_ref()
                .ok_or_else(|| async_graphql::Error::new("Google OAuth not configured"))?,
        };

        let (auth_url, state, pkce_verifier) = google.authorization_url();
        oauth_service
            .state_store
            .put(state.secret(), pkce_verifier)
            .await;

        Ok(StartOAuthPayload {
            auth_url: auth_url.to_string(),
        })
    }

    async fn complete_oauth(
        &self,
        ctx: &Context<'_>,
        provider: OAuthProvider,
        code: String,
        state: String,
    ) -> Result<CompleteOAuthPayload> {
        let oauth_service = ctx.data::<OAuthService>()?;

        let google = match provider {
            OAuthProvider::Google => oauth_service
                .google
                .as_ref()
                .ok_or_else(|| async_graphql::Error::new("Google OAuth not configured"))?,
        };

        let pkce_verifier = oauth_service
            .state_store
            .take(&state)
            .await
            .ok_or_else(|| async_graphql::Error::new("Invalid or expired OAuth state"))?;

        let token_response = google
            .exchange_code(AuthorizationCode::new(code), pkce_verifier)
            .await
            .map_err(|err| async_graphql::Error::new(err.to_string()))?;

        let expires_at = token_response
            .expires_in()
            .and_then(|duration| chrono::Duration::from_std(duration).ok())
            .map(|duration| (Utc::now() + duration).to_rfc3339());

        let user_info = google
            .fetch_user_info(token_response.access_token())
            .await
            .map_err(|err| {
                error!(error = %err, "Failed to fetch Google user info");
                async_graphql::Error::new("Failed to fetch user info from provider")
            })?;

        Ok(CompleteOAuthPayload {
            access_token: token_response.access_token().secret().to_string(),
            email: user_info.email,
            email_verified: user_info.email_verified,
            expires_at,
        })
    }
}

// GraphQL playground handler
pub async fn graphql_playground() -> impl IntoResponse {
    Html(playground_source(GraphQLPlaygroundConfig::new("/graphql")))
}

pub async fn graphql_handler(schema: Extension<ApiSchema>, req: GraphQLRequest) -> GraphQLResponse {
    schema.execute(req.into_inner()).await.into()
}

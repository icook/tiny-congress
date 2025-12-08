use crate::{
    auth::{issue_session_token, upsert_oauth_identity, OAuthService},
    config::AppConfig,
};
use async_graphql::http::{playground_source, GraphQLPlaygroundConfig};
use async_graphql::{Context, EmptySubscription, Enum, Object, Result, Schema, SimpleObject, ID};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::extract::Extension;
use axum::response::{Html, IntoResponse};
use chrono::Utc;
use oauth2::{AuthorizationCode, TokenResponse};
use serde_json::json;
use sqlx_postgres::PgPool;
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
    pub session_token: String,
    pub session_expires_at: String,
    pub user_id: ID,
    pub email: String,
    pub email_verified: bool,
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
        let pool = ctx.data::<PgPool>()?;
        let config = ctx.data::<AppConfig>()?;
        let jwt_secret = config
            .jwt
            .as_ref()
            .ok_or_else(|| async_graphql::Error::new("JWT secret not configured"))?;

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

        let user_info = google
            .fetch_user_info(token_response.access_token())
            .await
            .map_err(|err| {
                error!(error = %err, "Failed to fetch Google user info");
                async_graphql::Error::new("Failed to fetch user info from provider")
            })?;

        let user = upsert_oauth_identity(
            pool,
            "google",
            &user_info.sub,
            &user_info.email,
            user_info.email_verified,
            json!(&user_info),
        )
        .await
        .map_err(|err| {
            error!(error = %err, "Failed to upsert OAuth identity");
            async_graphql::Error::new("Failed to persist OAuth identity")
        })?;

        let (session_token, session_expires_at) =
            issue_session_token(&user, "google", &jwt_secret.secret).map_err(|err| {
                error!(error = %err, "Failed to issue session token");
                async_graphql::Error::new("Failed to issue session token")
            })?;

        Ok(CompleteOAuthPayload {
            session_token,
            session_expires_at: session_expires_at.to_rfc3339(),
            user_id: ID::from(user.id.to_string()),
            email: user.email,
            email_verified: user.email_verified,
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

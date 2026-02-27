use crate::build_info::BuildInfo;
use async_graphql::http::{playground_source, GraphQLPlaygroundConfig};
use async_graphql::{Context, EmptySubscription, Object, Result, Schema};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::extract::Extension;
use axum::response::{Html, IntoResponse};

/// The schema type with Query and Mutation roots
pub type ApiSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

/// Query root for the GraphQL API
pub struct QueryRoot;

#[Object]
impl QueryRoot {
    /// Returns build metadata for the running service
    #[allow(clippy::unused_async)]
    async fn build_info(&self, ctx: &Context<'_>) -> Result<BuildInfo> {
        Ok(ctx.data::<BuildInfo>()?.clone())
    }
}

/// Mutation root for the GraphQL API
pub struct MutationRoot;

#[Object]
impl MutationRoot {
    /// Placeholder mutation - returns the input string
    ///
    /// This exists because GraphQL requires at least one mutation.
    /// Replace with actual mutations as features are implemented.
    #[allow(clippy::unused_async)]
    async fn echo(&self, _ctx: &Context<'_>, message: String) -> String {
        message
    }
}

/// GraphQL playground handler - serves the interactive GraphQL IDE
#[allow(clippy::unused_async)]
pub async fn graphql_playground() -> impl IntoResponse {
    Html(playground_source(GraphQLPlaygroundConfig::new("/graphql")))
}

/// GraphQL request handler - executes GraphQL queries and mutations
pub async fn graphql_handler(schema: Extension<ApiSchema>, req: GraphQLRequest) -> GraphQLResponse {
    schema.execute(req.into_inner()).await.into()
}

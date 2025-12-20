//! GraphQL schema snapshot tests.
//!
//! These tests ensure the GraphQL API contract doesn't change unintentionally.
//! Run `cargo insta review` to inspect and approve intentional changes.

use async_graphql::{EmptySubscription, Schema};
use tinycongress_api::build_info::BuildInfoProvider;
use tinycongress_api::graphql::{MutationRoot, QueryRoot};

#[test]
fn graphql_schema() {
    let schema = Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(BuildInfoProvider::from_env())
        .finish();

    insta::assert_snapshot!(schema.sdl());
}

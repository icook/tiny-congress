//! Export the GraphQL schema as SDL for codegen.
//!
//! Usage: `cargo run --bin export_schema > ../web/schema.graphql`

#![allow(clippy::print_stdout)]

use async_graphql::{EmptySubscription, Schema};
use tinycongress_api::graphql::{MutationRoot, QueryRoot};

fn main() {
    let schema = Schema::build(QueryRoot, MutationRoot, EmptySubscription).finish();
    print!("{}", schema.sdl());
}

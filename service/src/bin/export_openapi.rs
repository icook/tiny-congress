//! Export the `OpenAPI` schema as JSON for codegen.
//!
//! Usage: `cargo run --bin export_openapi > ../web/openapi.json`

#![allow(clippy::print_stdout, clippy::expect_used)]

use tinycongress_api::rest::ApiDoc;
use utoipa::OpenApi;

fn main() {
    print!(
        "{}",
        ApiDoc::openapi()
            .to_pretty_json()
            .expect("OpenAPI JSON serialization failed")
    );
}

//! OpenAPI schema snapshot tests.
//!
//! These tests ensure the REST API contract doesn't change unintentionally.
//! Run `cargo insta review` to inspect and approve intentional changes.

use tinycongress_api::rest::ApiDoc;
use utoipa::OpenApi;

#[test]
fn openapi_schema() {
    let schema = ApiDoc::openapi();
    insta::assert_json_snapshot!(schema);
}

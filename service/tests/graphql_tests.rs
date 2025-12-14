use async_graphql::{EmptySubscription, Schema};
use serde_json::Value;
use tinycongress_api::build_info::BuildInfoProvider;
use tinycongress_api::graphql::{MutationRoot, QueryRoot};

async fn execute_query(query: &str) -> Value {
    let schema = Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(BuildInfoProvider::from_env())
        .finish();
    let response = schema.execute(query).await;
    serde_json::to_value(response).unwrap()
}

#[tokio::test]
async fn test_build_info_query() {
    let query = r#"
        {
            buildInfo {
                version
                gitSha
                buildTime
            }
        }
    "#;

    let result = execute_query(query).await;
    let data = &result["data"];
    assert!(data.is_object());

    let build_info = &data["buildInfo"];
    assert!(build_info.is_object());
    assert!(build_info["version"].is_string());
    assert!(build_info["gitSha"].is_string());
    assert!(build_info["buildTime"].is_string());
    assert!(!build_info["version"]
        .as_str()
        .unwrap_or_default()
        .is_empty());
    assert!(!build_info["gitSha"].as_str().unwrap_or_default().is_empty());
    assert!(!build_info["buildTime"]
        .as_str()
        .unwrap_or_default()
        .is_empty());
}

#[tokio::test]
async fn test_echo_mutation() {
    let mutation = r#"
        mutation {
            echo(message: "hello world")
        }
    "#;

    let result = execute_query(mutation).await;
    let data = &result["data"];
    assert!(data.is_object());

    let echo = &data["echo"];
    assert!(echo.is_string());
    assert_eq!(echo.as_str().unwrap(), "hello world");
}

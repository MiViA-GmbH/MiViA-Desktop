use mivia_ports::server::ServerError;
use mivia_server::ModelLabels;
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

use super::support::server;

#[tokio::test]
async fn fresh_labels_are_validated_cached_and_merged_over_the_snapshot() {
    let (mock, api, _) = server().await;
    let fresh = json!({
        "model": { "new-model-id": "New model" },
        "dashboard": { "title": "not ours" }
    });
    Mock::given(method("GET"))
        .and(path("/api/languages/en"))
        .and(header("if-none-match", "W/\"1\""))
        .respond_with(ResponseTemplate::new(304))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/languages/en"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "W/\"1\"")
                .set_body_json(fresh),
        )
        .mount(&mock)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let labels = ModelLabels::new(dir.path(), "app.example.invalid");
    let before = labels.served("en");
    assert!(
        before["model"]
            .as_object()
            .is_some_and(|models| !models.is_empty())
    );
    assert!(labels.refresh(&api, "en").await.unwrap());
    let served = labels.served("en");
    assert_eq!(served["model"]["new-model-id"], "New model");
    assert!(served.get("dashboard").is_none());
    assert_eq!(served["feedback"], before["feedback"]);
    assert!(!labels.refresh(&api, "en").await.unwrap());
}

#[tokio::test]
async fn labels_without_models_are_refused() {
    let (mock, api, _) = server().await;
    Mock::given(method("GET"))
        .and(path("/api/languages/de"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "model": {} })))
        .mount(&mock)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let labels = ModelLabels::new(dir.path(), "app.example.invalid");
    assert!(labels.refresh(&api, "de").await.is_err());
    Mock::given(method("GET"))
        .and(path("/api/languages/xx"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock)
        .await;
    let missing = labels.refresh(&api, "xx").await;
    assert!(matches!(
        missing,
        Err(mivia_server::LabelsError::Server(ServerError::Gone))
    ));
}

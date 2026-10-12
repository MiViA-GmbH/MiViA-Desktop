use mivia_ports::server::{ServerApi as _, ServerError};
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

#[tokio::test]
async fn catalogue_names_come_from_the_label_bundle_per_language() {
    let (mock, api, _) = server().await;
    let id = "00000000-0000-0000-0000-000000000007";
    Mock::given(method("GET"))
        .and(path("/api/settings/available-models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "id": id, "name": "GRAIN_X", "displayName": "Grain X", "accessType": "PUBLIC",
              "isDefaultVisible": true, "description_en": "Grain size." },
            { "id": "00000000-0000-0000-0000-000000000008", "name": "UNKNOWN", "displayName": "Unknown",
              "accessType": "PUBLIC", "isDefaultVisible": true }
        ])))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/api/models/{id}/customizations")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/api/models/00000000-0000-0000-0000-000000000008/customizations",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&mock)
        .await;
    for (lang, name) in [("en", "Grain size X"), ("de", "Korngröße X")] {
        Mock::given(method("GET"))
            .and(path(format!("/api/languages/{lang}")))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({ "model": { "GRAIN_X": name } })),
            )
            .mount(&mock)
            .await;
    }
    let dir = tempfile::tempdir().unwrap();
    let api = api.with_labels(ModelLabels::new(dir.path(), "app.example.invalid"));
    let models = api.catalogue().await.unwrap();
    assert_eq!(
        (models[0].name_en.as_str(), models[0].name_de.as_str()),
        ("Grain size X", "Korngröße X")
    );
    assert_eq!(
        (
            models[0].description_en.as_str(),
            models[0].description_de.as_str()
        ),
        ("Grain size.", "Grain size.")
    );
    assert_eq!(
        (models[1].name_en.as_str(), models[1].name_de.as_str()),
        ("Unknown", "Unknown")
    );
}

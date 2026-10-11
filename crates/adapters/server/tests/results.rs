use async_trait::async_trait;
use mivia_domain::ids::Fingerprint;
use mivia_ports::fs::{ByteSink, FsError, Staged};
use mivia_ports::server::{Download, ServerApi, ServerError};
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

use super::support::{KEY, server};

const JOB: &str = "5c3d2e1f-0000-4000-8000-000000000001";
const IMAGE: &str = "01a12b41-61d1-74b2-9e9e-fa7640849486.png";

struct Collect(Vec<u8>);

#[async_trait]
impl ByteSink for Collect {
    async fn write(&mut self, chunk: Vec<u8>) -> Result<(), FsError> {
        self.0.extend(chunk);
        Ok(())
    }

    async fn finish(self: Box<Self>) -> Result<Staged, FsError> {
        Ok(Staged {
            size: u64::try_from(self.0.len()).unwrap(),
            sha256: Fingerprint([0; 32]),
            head: self.0.iter().take(8).copied().collect(),
        })
    }
}

#[tokio::test]
async fn results_are_the_array_of_the_job_detail() {
    let (mock, api, _) = server().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v2/jobs/{JOB}")))
        .and(header("authorization", KEY))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": JOB,
            "result": { "feedback": null, "results": [{ "type": "message", "label": "ok" }] }
        })))
        .mount(&mock)
        .await;
    let results = api.job_results(JOB).await.unwrap().unwrap();
    assert_eq!(results, r#"[{"label":"ok","type":"message"}]"#);
}

#[tokio::test]
async fn jobs_without_results_or_not_ours_give_none() {
    let (mock, api, _) = server().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v2/jobs/{JOB}")))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "id": JOB, "result": null })),
        )
        .mount(&mock)
        .await;
    assert_eq!(api.job_results(JOB).await, Ok(None));
    let other = "5c3d2e1f-0000-4000-8000-000000000002";
    Mock::given(method("GET"))
        .and(path(format!("/api/v2/jobs/{other}")))
        .respond_with(ResponseTemplate::new(403))
        .mount(&mock)
        .await;
    assert_eq!(api.job_results(other).await, Ok(None));
    assert!(matches!(
        api.job_results("not-an-id").await,
        Err(ServerError::BadRequest { .. })
    ));
}

#[tokio::test]
async fn the_image_is_fetched_from_the_redirect_without_the_access_key() {
    let (mock, api, _) = server().await;
    let target = format!("{}/storage/{IMAGE}", mock.uri());
    Mock::given(method("GET"))
        .and(path(format!("/api/jobs/results/{JOB}/{IMAGE}")))
        .and(header("authorization", KEY))
        .respond_with(ResponseTemplate::new(302).insert_header("location", target.as_str()))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/storage/{IMAGE}")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"\x89PNG....".to_vec()))
        .mount(&mock)
        .await;
    let download = api
        .download_result_image(JOB, IMAGE, Box::new(Collect(Vec::new())))
        .await
        .unwrap();
    let Download::Staged(staged) = download else {
        panic!("expired");
    };
    assert_eq!(staged.size, 8);
    let requests = mock.received_requests().await.unwrap();
    let fetched = requests
        .iter()
        .find(|r| r.url.path().starts_with("/storage/"))
        .unwrap();
    assert!(!fetched.headers.contains_key("authorization"));
}

#[tokio::test]
async fn missing_images_and_foreign_jobs_are_expired() {
    for status in [403, 404] {
        let (mock, api, _) = server().await;
        Mock::given(method("GET"))
            .and(path(format!("/api/jobs/results/{JOB}/{IMAGE}")))
            .respond_with(ResponseTemplate::new(status))
            .mount(&mock)
            .await;
        let download = api
            .download_result_image(JOB, IMAGE, Box::new(Collect(Vec::new())))
            .await;
        assert_eq!(download, Ok(Download::Expired));
    }
}

#[tokio::test]
async fn file_names_that_could_leave_the_route_are_refused() {
    let (_mock, api, _) = server().await;
    for name in ["../x.png", "a/b.png", ""] {
        let refused = api
            .download_result_image(JOB, name, Box::new(Collect(Vec::new())))
            .await;
        assert!(
            matches!(refused, Err(ServerError::BadRequest { .. })),
            "{name}"
        );
    }
}

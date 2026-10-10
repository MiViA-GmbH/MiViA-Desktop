use async_trait::async_trait;
use mivia_domain::ids::Fingerprint;
use mivia_domain::naming::Language;
use mivia_domain::output::Format;
use mivia_ports::fs::{ByteSink, FsError, Staged};
use mivia_ports::server::{Download, ReportProgress, ReportRequest, ServerApi, ServerError};
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

use super::support::{KEY, server};

const REPORT: &str = "5c3d2e1f-0000-4000-8000-000000000001";

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

fn request() -> ReportRequest {
    ReportRequest {
        format: Format::Pdf,
        job_ids: vec!["9d2e4f6a-0000-4000-8000-0000000000aa".to_owned()],
        timezone: "Europe/Berlin".to_owned(),
        include_images: false,
        language: Language::De,
        extras: None,
    }
}

#[tokio::test]
async fn requests_post_the_body_in_the_report_language() {
    let (mock, api, _) = server().await;
    Mock::given(method("POST"))
        .and(path("/api/reports/pdf"))
        .and(header("authorization", KEY))
        .and(header("accept-language", "de"))
        .respond_with(
            ResponseTemplate::new(202)
                .set_body_json(json!({ "reportId": REPORT, "status": "PENDING" })),
        )
        .mount(&mock)
        .await;
    assert_eq!(api.request_report(request()).await.unwrap(), REPORT);
    let sent: serde_json::Value = mock.received_requests().await.unwrap()[0]
        .body_json()
        .unwrap();
    assert_eq!(sent["timezone"], json!("Europe/Berlin"));
    assert_eq!(sent["includeImages"], json!(false));
}

#[tokio::test]
async fn request_refusals_map_to_their_classes() {
    let cases = [
        (
            404,
            json!({ "message": "No requested job has a result" }),
            "NotFound",
        ),
        (
            403,
            json!({ "message": "One or more jobs are not available" }),
            "Forbidden",
        ),
        (
            429,
            json!({ "message": "Too many reports in flight" }),
            "RateLimited",
        ),
    ];
    for (status, body, expected) in cases {
        let (mock, api, _) = server().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(status).set_body_json(body))
            .mount(&mock)
            .await;
        let error = api.request_report(request()).await.unwrap_err();
        assert!(
            format!("{error:?}").starts_with(expected),
            "{status}: {error:?}"
        );
    }
}

#[tokio::test]
async fn status_reads_too_large_failures_and_unknown_reports() {
    let (mock, api, _) = server().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/reports/{REPORT}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": REPORT, "status": "FAILED", "errorCode": "REPORT_TOO_LARGE",
            "createdAt": "2026-10-09T10:00:00.000Z"
        })))
        .mount(&mock)
        .await;
    assert_eq!(
        api.report_status(REPORT).await,
        Ok(ReportProgress::Failed { too_large: true })
    );
    let missing = "5c3d2e1f-0000-4000-8000-000000000002";
    assert_eq!(api.report_status(missing).await, Err(ServerError::Gone));
}

#[tokio::test]
async fn downloads_follow_the_link_without_the_access_key() {
    let (mock, api, _) = server().await;
    let file = format!("{}/files/{REPORT}.pdf", mock.uri());
    Mock::given(method("GET"))
        .and(path(format!("/api/reports/{REPORT}/download")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "url": file })))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/files/{REPORT}.pdf")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"%PDF-1.7 body".to_vec()))
        .mount(&mock)
        .await;
    let download = api
        .download_report(REPORT, Box::new(Collect(Vec::new())))
        .await
        .unwrap();
    let Download::Staged(staged) = download else {
        panic!("expired");
    };
    assert_eq!(staged.size, 13);
    assert!(staged.head.starts_with(b"%PDF-"));
    let requests = mock.received_requests().await.unwrap();
    let fetched = requests
        .iter()
        .find(|r| r.url.path().starts_with("/files/"))
        .unwrap();
    assert!(!fetched.headers.contains_key("authorization"));
}

#[tokio::test]
async fn expired_links_and_gone_objects_are_expired() {
    for (body, file_status) in [
        (json!({ "status": "expired" }), 200),
        (json!({ "url": "FILE" }), 403),
    ] {
        let (mock, api, _) = server().await;
        let file = format!("{}/files/x.pdf", mock.uri());
        let body = serde_json::from_str(&body.to_string().replace("FILE", &file)).unwrap();
        Mock::given(method("GET"))
            .and(path(format!("/api/reports/{REPORT}/download")))
            .respond_with(ResponseTemplate::new(200).set_body_json::<serde_json::Value>(body))
            .mount(&mock)
            .await;
        Mock::given(method("GET"))
            .and(path("/files/x.pdf"))
            .respond_with(ResponseTemplate::new(file_status))
            .mount(&mock)
            .await;
        let download = api
            .download_report(REPORT, Box::new(Collect(Vec::new())))
            .await;
        assert_eq!(download, Ok(Download::Expired));
    }
}

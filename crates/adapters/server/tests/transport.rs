use std::time::SystemTime;

use async_trait::async_trait;
use mivia_ports::fs::{ChunkSource, FsError};
use mivia_ports::server::{ServerApi, ServerError, ServerJobStatus, UploadRequest};
use serde_json::json;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

use super::support::{IMAGE, KEY, OTHER_IMAGE, api_for, server, submit};

struct Chunks {
    chunks: Vec<Vec<u8>>,
    hang: bool,
}

#[async_trait]
impl ChunkSource for Chunks {
    async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, FsError> {
        if self.chunks.is_empty() && self.hang {
            std::future::pending::<()>().await;
        }
        Ok((!self.chunks.is_empty()).then(|| self.chunks.remove(0)))
    }
}

pub(crate) fn bytes_upload(bytes: &[u8]) -> UploadRequest {
    UploadRequest {
        file_name: "image-1.png".to_owned(),
        len: u64::try_from(bytes.len()).unwrap(),
        source: Box::new(Chunks {
            chunks: vec![bytes.to_vec()],
            hang: false,
        }),
    }
}

pub(crate) fn http_date(at: SystemTime) -> String {
    httpdate::fmt_http_date(at)
}

fn image(id: &str, duplicate: bool) -> serde_json::Value {
    json!({ "id": id, "createdAt": "2026-10-09T10:00:00.000Z", "orginalFilename": "image-1.png",
        "thumbnailUrl": "https://example.invalid/t", "duplicate": duplicate })
}

fn job(id: &str, image: &str, status: &str) -> serde_json::Value {
    json!({ "id": id, "imageId": image, "modelId": super::support::MODEL, "status": status,
        "result": {}, "createdAt": "2026-10-09T10:00:00.000Z", "startedAt": "2026-10-09T10:00:00.000Z",
        "finishedAt": "2026-10-09T10:00:00.000Z", "archived": false, "outdated": false })
}

#[tokio::test]
async fn uploads_stream_one_files_field_with_a_length_and_the_key() {
    let (mock, api, _) = server().await;
    Mock::given(method("POST"))
        .and(path("/api/image"))
        .and(header("authorization", KEY))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([image(IMAGE, true)])))
        .mount(&mock)
        .await;
    let image = api.upload(bytes_upload(b"pixels")).await.unwrap();
    assert_eq!((image.image_id.as_str(), image.duplicate), (IMAGE, true));
    let requests = mock.received_requests().await.unwrap();
    let request = &requests[0];
    assert!(request.headers.get("content-length").is_some());
    assert!(
        request
            .headers
            .get("user-agent")
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("MiViA-Desktop/2.0.0")
    );
    let body = String::from_utf8_lossy(&request.body);
    assert_eq!(body.matches("name=\"files\"").count(), 1);
    assert!(body.contains("pixels"));
}

#[tokio::test]
async fn submits_carry_the_idempotency_key_and_jobs_map_by_image() {
    let (mock, api, _) = server().await;
    let jobs = json!([
        job(
            "4e9a1f2c-0000-4000-8000-000000000002",
            OTHER_IMAGE,
            "CACHED"
        ),
        job("4e9a1f2c-0000-4000-8000-000000000001", IMAGE, "PENDING"),
    ]);
    Mock::given(method("POST"))
        .and(path("/api/jobs"))
        .and(header(
            "idempotency-key",
            "00000000-0000-0000-0000-000000000007",
        ))
        .respond_with(ResponseTemplate::new(201).set_body_json(jobs))
        .mount(&mock)
        .await;
    let created = api.submit(submit(&[IMAGE, OTHER_IMAGE])).await.unwrap();
    let by_image: Vec<(Option<String>, ServerJobStatus)> = created
        .into_iter()
        .map(|job| (job.image_id, job.status))
        .collect();
    assert!(by_image.contains(&(Some(IMAGE.to_owned()), ServerJobStatus::Pending)));
    assert!(by_image.contains(&(Some(OTHER_IMAGE.to_owned()), ServerJobStatus::Cached)));
    let requests = mock.received_requests().await.unwrap();
    let sent: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(sent["source"], "API");
    assert_eq!(sent["imageIds"], json!([IMAGE, OTHER_IMAGE]));
}

#[tokio::test]
async fn a_null_licence_means_no_licence() {
    let (mock, api, _) = server().await;
    Mock::given(method("GET"))
        .and(path("/api/users/license"))
        .respond_with(ResponseTemplate::new(200).set_body_raw("null", "application/json"))
        .mount(&mock)
        .await;
    assert_eq!(api.license().await, Err(ServerError::LicenseInvalid));
}

#[tokio::test]
async fn json_bodies_above_eight_mebibytes_are_contract_errors() {
    let (mock, api, _) = server().await;
    let huge = format!("[{}]", " ".repeat(9 * 1024 * 1024));
    Mock::given(method("GET"))
        .and(path("/api/v2/jobs/status"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(huge, "application/json"))
        .mount(&mock)
        .await;
    let ids = vec!["4e9a1f2c-0000-4000-8000-000000000001".to_owned()];
    assert_eq!(api.job_statuses(ids).await, Err(ServerError::Contract));
}

async fn silent_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let uri = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buffer = vec![0; 64 * 1024];
        while socket.read(&mut buffer).await.is_ok_and(|n| n > 0) {}
    });
    uri
}

#[tokio::test(start_paused = true)]
async fn a_stalled_upload_body_is_dropped_after_a_minute() {
    let uri = silent_server().await;
    let (api, _) = api_for(&uri);
    let request = UploadRequest {
        file_name: "slow.png".to_owned(),
        len: 10,
        source: Box::new(Chunks {
            chunks: vec![b"half".to_vec()],
            hang: true,
        }),
    };
    assert_eq!(api.upload(request).await, Err(ServerError::StallTimeout));
}

#[tokio::test(start_paused = true)]
async fn an_unanswered_upload_is_dropped_after_five_minutes() {
    let uri = silent_server().await;
    let (api, _) = api_for(&uri);
    assert_eq!(
        api.upload(bytes_upload(b"pixels")).await,
        Err(ServerError::StallTimeout)
    );
}

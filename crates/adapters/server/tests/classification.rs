use std::time::{Duration, SystemTime};

use mivia_domain::hold::KeyProblem;
use mivia_ports::auth::AuthError;
use mivia_ports::server::{ServerApi, ServerError};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::support::{IMAGE, OTHER_IMAGE, server, submit};
use super::transport::bytes_upload;

const JOB: &str = "4e9a1f2c-0000-4000-8000-000000000001";

fn error_body(code: &str, message: &str) -> serde_json::Value {
    json!({ "statusCode": 400, "code": code, "message": message })
}

async fn status_error(template: ResponseTemplate) -> ServerError {
    let (mock, api, _) = server().await;
    Mock::given(method("GET"))
        .and(path("/api/v2/jobs/status"))
        .respond_with(template)
        .mount(&mock)
        .await;
    api.job_statuses(vec![JOB.to_owned()]).await.unwrap_err()
}

async fn submit_error(template: ResponseTemplate) -> ServerError {
    let (mock, api, _) = server().await;
    Mock::given(method("POST"))
        .and(path("/api/jobs"))
        .respond_with(template)
        .mount(&mock)
        .await;
    api.submit(submit(&[IMAGE, OTHER_IMAGE])).await.unwrap_err()
}

async fn upload_error(template: ResponseTemplate) -> ServerError {
    let (mock, api, _) = server().await;
    Mock::given(method("POST"))
        .and(path("/api/image"))
        .respond_with(template)
        .mount(&mock)
        .await;
    api.upload(bytes_upload(b"bytes")).await.unwrap_err()
}

#[tokio::test]
async fn statuses_map_to_the_outcome_table() {
    let table = [
        (
            ResponseTemplate::new(500),
            ServerError::Unavailable { status: 500 },
        ),
        (
            ResponseTemplate::new(502),
            ServerError::Unavailable { status: 502 },
        ),
        (
            ResponseTemplate::new(503),
            ServerError::Unavailable { status: 503 },
        ),
        (
            ResponseTemplate::new(408),
            ServerError::Unavailable { status: 408 },
        ),
        (
            ResponseTemplate::new(302).insert_header("location", "/login"),
            ServerError::Redirect,
        ),
        (ResponseTemplate::new(404), ServerError::Gone),
        (ResponseTemplate::new(410), ServerError::Gone),
        (
            ResponseTemplate::new(422).set_body_json(error_body("VALIDATION", "bad")),
            ServerError::BadRequest {
                code: "VALIDATION".to_owned(),
            },
        ),
        (
            ResponseTemplate::new(400)
                .set_body_json(error_body("IDEMPOTENCY_KEY_REUSED", "reused")),
            ServerError::BadRequest {
                code: "IDEMPOTENCY_KEY_REUSED".to_owned(),
            },
        ),
        (
            ResponseTemplate::new(403).set_body_json(error_body("ACCESS_DENIED", "denied")),
            ServerError::Forbidden {
                code: "ACCESS_DENIED".to_owned(),
            },
        ),
        (
            ResponseTemplate::new(403)
                .set_body_json(json!({ "statusCode": 403, "message": "License is not valid" })),
            ServerError::LicenseInvalid,
        ),
        (
            ResponseTemplate::new(200).set_body_string("<html>sign in</html>"),
            ServerError::Contract,
        ),
    ];
    for (template, expected) in table {
        let status = template.clone();
        assert_eq!(status_error(status).await, expected, "{template:?}");
    }
}

#[tokio::test]
async fn retry_after_is_read_in_both_forms_and_clamped() {
    let seconds = ResponseTemplate::new(429).insert_header("retry-after", "120");
    assert_eq!(
        status_error(seconds).await,
        ServerError::RateLimited {
            retry_after_ms: 120_000
        }
    );
    let now = SystemTime::now();
    let date = httpdate_text(now);
    let later = httpdate_text(now + Duration::from_secs(90));
    let dated = ResponseTemplate::new(429)
        .insert_header("date", date.as_str())
        .insert_header("retry-after", later.as_str());
    assert_eq!(
        status_error(dated).await,
        ServerError::RateLimited {
            retry_after_ms: 90_000
        }
    );
    let missing = ResponseTemplate::new(429);
    assert_eq!(
        status_error(missing).await,
        ServerError::RateLimited {
            retry_after_ms: 30_000
        }
    );
    let huge = ResponseTemplate::new(429).insert_header("retry-after", "86400");
    assert_eq!(
        status_error(huge).await,
        ServerError::RateLimited {
            retry_after_ms: 3_600_000
        }
    );
}

fn httpdate_text(at: SystemTime) -> String {
    super::transport::http_date(at)
}

#[tokio::test]
async fn submit_errors_name_the_missing_image_and_flag_in_progress_keys() {
    let missing = ResponseTemplate::new(404).set_body_json(
        json!({ "statusCode": 404, "message": format!("Image {OTHER_IMAGE} not found") }),
    );
    assert_eq!(
        submit_error(missing).await,
        ServerError::NotFound {
            image_id: Some(OTHER_IMAGE.to_owned())
        }
    );
    let unnamed = ResponseTemplate::new(404).set_body_json(json!({ "message": "Image not found" }));
    assert_eq!(
        submit_error(unnamed).await,
        ServerError::NotFound { image_id: None }
    );
    let busy =
        ResponseTemplate::new(409).set_body_json(error_body("IDEMPOTENCY_KEY_IN_PROGRESS", "busy"));
    assert_eq!(submit_error(busy).await, ServerError::InProgress);
    let license = ResponseTemplate::new(403).set_body_json(error_body("LICENSE_INVALID", "x"));
    assert_eq!(submit_error(license).await, ServerError::LicenseInvalid);
}

#[tokio::test]
async fn upload_errors_tell_rejected_files_from_proxy_limits() {
    let rejected = ResponseTemplate::new(400).set_body_json(json!({
        "code": "IMAGE_UPLOAD_REJECTED", "message": "1 of 1 files rejected",
        "errors": [{ "code": "IMAGE_RESOLUTION_TOO_LOW", "filename": "a.png", "message": "small" }]
    }));
    assert!(matches!(
        upload_error(rejected).await,
        ServerError::ImageRejected { .. }
    ));
    let json_413 =
        ResponseTemplate::new(413).set_body_json(json!({ "statusCode": 413, "message": "big" }));
    assert_eq!(
        upload_error(json_413).await,
        ServerError::TooLarge { by_proxy: false }
    );
    let html_413 =
        ResponseTemplate::new(413).set_body_string("<html>413 Request Entity Too Large</html>");
    assert_eq!(
        upload_error(html_413).await,
        ServerError::TooLarge { by_proxy: true }
    );
    let license = ResponseTemplate::new(403).set_body_json(error_body("LICENSE_INVALID", "x"));
    assert_eq!(upload_error(license).await, ServerError::LicenseInvalid);
}

#[tokio::test]
async fn an_unauthorized_answer_is_confirmed_once_and_then_held() {
    let (mock, api, auth) = server().await;
    Mock::given(method("GET"))
        .and(path("/api/v2/jobs/status"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&mock)
        .await;
    assert_eq!(
        api.job_statuses(vec![JOB.to_owned()]).await,
        Err(ServerError::AfterSend)
    );
    assert_eq!(auth.checks.load(std::sync::atomic::Ordering::Relaxed), 2);
    *auth.answer.lock() = Err(AuthError::Rejected(KeyProblem::AccountBlocked));
    assert_eq!(
        api.job_statuses(vec![JOB.to_owned()]).await,
        Err(ServerError::Unauthorized(KeyProblem::AccountBlocked))
    );
}

#[tokio::test]
async fn a_captive_portal_page_on_the_version_probe_is_a_redirect_hold() {
    let (mock, api, _) = server().await;
    Mock::given(method("GET"))
        .and(path("/api/version"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>Wi-Fi login</html>"))
        .mount(&mock)
        .await;
    assert_eq!(api.version().await, Err(ServerError::Redirect));
}

#[tokio::test]
async fn a_closed_port_is_unreachable() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let uri = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let (api, _) = super::support::api_for(&uri);
    assert_eq!(api.version().await, Err(ServerError::Unreachable));
}

#[tokio::test]
async fn a_missing_scope_is_a_key_problem_without_a_new_check() {
    let missing = || {
        ResponseTemplate::new(403).set_body_json(json!({
            "statusCode": 403,
            "code": "API_KEY_SCOPE_MISSING",
            "message": "API key lacks scope jobs:write",
            "scope": "jobs:write"
        }))
    };
    let held = ServerError::Unauthorized(KeyProblem::MissingScopes);
    assert_eq!(upload_error(missing()).await, held);
    assert_eq!(submit_error(missing()).await, held);
    let (mock, api, auth) = server().await;
    Mock::given(method("GET"))
        .and(path("/api/v2/jobs/status"))
        .respond_with(missing())
        .mount(&mock)
        .await;
    assert_eq!(api.job_statuses(vec![JOB.to_owned()]).await, Err(held));
    assert_eq!(auth.checks.load(std::sync::atomic::Ordering::Relaxed), 0);
}

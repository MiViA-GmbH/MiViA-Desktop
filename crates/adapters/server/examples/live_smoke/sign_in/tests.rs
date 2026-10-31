use std::path::{Path, PathBuf};
use std::time::Duration;

use mivia_api::{DeviceCode, IssuedAccessKey, OauthAccount, OauthClient, OauthError, OauthToken};
use mivia_server::build_clients;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tempfile::TempDir;
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use super::scrub::SignInRecorder;
use super::{after_refusal, run};

const PREVIOUS: &str = "018f2c6e-0000-7000-8000-00000000000a";
const ISSUED: &str = "018f2c6e-0000-7000-8000-00000000000b";
const REAL_KEY: &str = "mv-RealIssuedKeyRealIssuedKeyRealIssuedKey";

struct Run {
    mock: MockServer,
    folder: TempDir,
    fixtures: PathBuf,
    state: PathBuf,
}

async fn mount(mock: &MockServer, verb: &str, route: &str, reply: ResponseTemplate) {
    Mock::given(method(verb))
        .and(path(route))
        .respond_with(reply)
        .mount(mock)
        .await;
}

fn json_reply(status: u16, body: &Value) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(body)
}

async fn serve_until_device_code(mock: &MockServer) {
    mount(
        mock,
        "GET",
        "/api/oauth-clients/by-app/mivia-desktop",
        json_reply(200, &json!({ "clientId": "client-1" })),
    )
    .await;
    let device = json!({
        "device_code": "real-device-code",
        "user_code": "REAL-CODE",
        "verification_uri": format!("{}/device", mock.uri()),
        "verification_uri_complete": format!("{}/device?user_code=REAL-CODE", mock.uri()),
        "expires_in": 600,
        "interval": 1,
    });
    Mock::given(method("POST"))
        .and(path("/api/auth/device/code"))
        .and(body_string_contains("client_id=client-1"))
        .and(body_string_contains("api-keys%3Aissue"))
        .respond_with(json_reply(200, &device))
        .mount(mock)
        .await;
}

async fn serve_pending_then_token(mock: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/api/auth/oauth2/token"))
        .respond_with(json_reply(
            400,
            &json!({ "error": "authorization_pending", "error_description": "still pending" }),
        ))
        .up_to_n_times(1)
        .mount(mock)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/auth/oauth2/token"))
        .and(body_string_contains("device_code=real-device-code"))
        .respond_with(json_reply(
            200,
            &json!({ "access_token": "real-token", "token_type": "Bearer", "scope": "openid" }),
        ))
        .mount(mock)
        .await;
}

async fn serve_after_token(mock: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/auth/oauth2/userinfo"))
        .and(header("authorization", "Bearer real-token"))
        .respond_with(json_reply(
            200,
            &json!({ "sub": "account-1", "name": "Real Person", "email": "real@mivia.ai" }),
        ))
        .mount(mock)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/api-keys/issue"))
        .and(header("authorization", "Bearer real-token"))
        .respond_with(json_reply(
            201,
            &json!({ "id": ISSUED, "name": "whatever", "start": "mv-RealIs", "code": REAL_KEY }),
        ))
        .mount(mock)
        .await;
}

async fn happy_server() -> Run {
    let mock = MockServer::start().await;
    serve_until_device_code(&mock).await;
    serve_pending_then_token(&mock).await;
    serve_after_token(&mock).await;
    let folder = tempfile::tempdir().unwrap();
    let fixtures = folder.path().join("fixtures");
    let state = folder.path().join("target").join("key-id");
    Run {
        mock,
        folder,
        fixtures,
        state,
    }
}

async fn drive(run_on: &Run) -> Result<(), String> {
    let clients = build_clients("MiViA-Desktop/2.0.0 (Windows 10.0.26200; stable)").unwrap();
    let mut recorder = SignInRecorder {
        dir: Some(run_on.fixtures.clone()),
        host: run_on.mock.address().to_string(),
        held: Vec::new(),
    };
    run(
        &run_on.mock.uri(),
        &clients.api,
        &mut recorder,
        &run_on.state,
    )
    .await
    .map_err(|e| e.to_string())
}

fn decode<T: DeserializeOwned>(dir: &Path, operation: &str, stem: &str) -> T {
    let bytes = std::fs::read(dir.join(operation).join(format!("{stem}.json"))).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn issue_bodies(mock: &MockServer) -> Vec<Value> {
    let requests: Vec<Request> = mock.received_requests().await.unwrap();
    requests
        .iter()
        .filter(|request| request.url.path() == "/api/api-keys/issue")
        .map(|request| serde_json::from_slice(&request.body).unwrap())
        .collect()
}

#[tokio::test]
async fn a_run_records_every_call_and_the_files_decode_into_their_types() {
    let server = happy_server().await;
    drive(&server).await.unwrap();
    let dir = &server.fixtures;
    let client: OauthClient = decode(dir, "oauthClientByApp", "200");
    assert_eq!(client.client_id, "dgVNLqgQfTzZkAEzGUIljVHnKhyTBlYS");
    let device: DeviceCode = decode(dir, "oauthDeviceCode", "200");
    assert_eq!(device.device_code, "fixture-device-code");
    let pending: OauthError = decode(dir, "oauthToken", "400-pending");
    assert_eq!(pending.error, "authorization_pending");
    let token: OauthToken = decode(dir, "oauthToken", "200");
    assert_eq!(token.access_token, "fixture-access-token");
    let account: OauthAccount = decode(dir, "oauthUserinfo", "200");
    assert_eq!(account.sub, "account-1");
    let issued: IssuedAccessKey = decode(dir, "accessKeyIssue", "201");
    assert_eq!(issued.id, ISSUED);
    assert!(issued.code.starts_with("mv-fixture"));
}

#[tokio::test]
async fn nothing_real_reaches_the_recorded_files() {
    let server = happy_server().await;
    drive(&server).await.unwrap();
    for operation in std::fs::read_dir(&server.fixtures).unwrap() {
        for file in std::fs::read_dir(operation.unwrap().path()).unwrap() {
            let text = std::fs::read_to_string(file.unwrap().path()).unwrap();
            for real in ["real-", "REAL-", "Real", "@mivia.ai", "127.0.0.1"] {
                assert!(!text.contains(real), "{real} in {text}");
            }
        }
    }
}

#[tokio::test]
async fn the_first_run_sends_no_replaces_and_a_later_one_sends_the_previous_id() {
    let server = happy_server().await;
    drive(&server).await.unwrap();
    drive(&server).await.unwrap();
    let bodies = issue_bodies(&server.mock).await;
    assert_eq!(bodies.len(), 2);
    assert_eq!(bodies[0].get("replaces"), None);
    assert_eq!(bodies[1]["replaces"], ISSUED);
    assert_eq!(bodies[0]["name"], "MiViA Desktop live smoke");
}

#[tokio::test]
async fn an_id_is_kept_for_the_next_run_even_when_scrubbing_stops_this_one() {
    let server = MockServer::start().await;
    serve_until_device_code(&server).await;
    serve_pending_then_token(&server).await;
    mount(
        &server,
        "GET",
        "/api/auth/oauth2/userinfo",
        json_reply(200, &json!({ "sub": "account-1" })),
    )
    .await;
    mount(
        &server,
        "POST",
        "/api/api-keys/issue",
        json_reply(
            201,
            &json!({ "id": ISSUED, "code": REAL_KEY, "enabled": true, "note": REAL_KEY }),
        ),
    )
    .await;
    let folder = tempfile::tempdir().unwrap();
    let on = Run {
        mock: server,
        fixtures: folder.path().join("fixtures"),
        state: folder.path().join("key-id"),
        folder,
    };
    let stopped = drive(&on).await.unwrap_err();
    assert!(stopped.contains("accessKeyIssue"), "{stopped}");
    assert!(!stopped.contains(REAL_KEY), "{stopped}");
    assert!(!on.fixtures.join("accessKeyIssue").exists());
    assert_eq!(std::fs::read_to_string(&on.state).unwrap(), ISSUED);
    assert!(on.folder.path().exists());
}

#[tokio::test]
async fn a_saved_id_goes_out_as_replaces_and_anything_else_in_the_file_does_not() {
    for (saved, sent) in [(PREVIOUS, Some(PREVIOUS)), ("not an id", None), ("", None)] {
        let server = happy_server().await;
        std::fs::create_dir_all(server.state.parent().unwrap()).unwrap();
        std::fs::write(&server.state, saved).unwrap();
        drive(&server).await.unwrap();
        let bodies = issue_bodies(&server.mock).await;
        assert_eq!(bodies[0].get("replaces").and_then(Value::as_str), sent);
    }
}

#[tokio::test]
async fn a_server_without_the_lookup_stops_before_any_code_is_requested() {
    let mock = MockServer::start().await;
    mount(
        &mock,
        "GET",
        "/api/oauth-clients/by-app/mivia-desktop",
        ResponseTemplate::new(200).set_body_string("<html>app</html>"),
    )
    .await;
    let folder = tempfile::tempdir().unwrap();
    let on = Run {
        mock,
        fixtures: folder.path().join("fixtures"),
        state: folder.path().join("key-id"),
        folder,
    };
    let stopped = drive(&on).await.unwrap_err();
    assert!(stopped.contains("oauthClientByApp"), "{stopped}");
    assert_eq!(on.mock.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_denial_stops_the_wait_and_issues_no_key() {
    let mock = MockServer::start().await;
    serve_until_device_code(&mock).await;
    mount(
        &mock,
        "POST",
        "/api/auth/oauth2/token",
        json_reply(400, &json!({ "error": "access_denied" })),
    )
    .await;
    let folder = tempfile::tempdir().unwrap();
    let on = Run {
        mock,
        fixtures: folder.path().join("fixtures"),
        state: folder.path().join("key-id"),
        folder,
    };
    let stopped = drive(&on).await.unwrap_err();
    assert!(stopped.contains("access_denied"), "{stopped}");
    assert_eq!(issue_bodies(&on.mock).await, Vec::<Value>::new());
    assert!(!on.state.exists());
}

#[test]
fn slow_down_grows_the_wait_and_pending_keeps_it() {
    let wait = Duration::from_secs(5);
    assert_eq!(
        after_refusal("authorization_pending", wait).unwrap(),
        Duration::from_secs(5)
    );
    assert_eq!(
        after_refusal("slow_down", wait).unwrap(),
        Duration::from_secs(10)
    );
    for ending in [
        "access_denied",
        "expired_token",
        "invalid_grant",
        "invalid_client",
    ] {
        assert!(
            after_refusal(ending, wait).is_err(),
            "{ending} kept waiting"
        );
    }
}

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mivia_server::SignInError;
use parking_lot::Mutex;
use secrecy::ExposeSecret;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

use super::{ISSUED_KEY, TOKEN_PATH, api, form, serve_client};

const USER_CODE: &str = "WDJB-MJHT";

struct Polls {
    answers: Mutex<VecDeque<ResponseTemplate>>,
    at: Arc<Mutex<Vec<Instant>>>,
}

impl Respond for Polls {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        self.at.lock().push(Instant::now());
        self.answers.lock().pop_front().unwrap()
    }
}

fn poll_error(code: &str) -> ResponseTemplate {
    ResponseTemplate::new(400).set_body_json(json!({ "error": code }))
}

fn poll_token() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({ "access_token": "tok-1" }))
}

async fn serve_device(mock: &MockServer, polls: Vec<ResponseTemplate>) -> Arc<Mutex<Vec<Instant>>> {
    serve_client(mock).await;
    Mock::given(method("POST"))
        .and(path("/api/auth/device/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "device_code": "device-1",
            "user_code": USER_CODE,
            "verification_uri": "https://evil.example/device",
            "expires_in": 600,
            "interval": 1,
        })))
        .mount(mock)
        .await;
    let at = Arc::new(Mutex::new(Vec::new()));
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(Polls {
            answers: Mutex::new(polls.into()),
            at: Arc::clone(&at),
        })
        .mount(mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/auth/oauth2/userinfo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "sub": "account-1" })))
        .mount(mock)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/api-keys/issue"))
        .respond_with(
            ResponseTemplate::new(201).set_body_json(json!({ "id": "key-1", "code": ISSUED_KEY })),
        )
        .mount(mock)
        .await;
    at
}

async fn keys_issued(mock: &MockServer) -> usize {
    mock.received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| request.url.path() == "/api/api-keys/issue")
        .count()
}

#[tokio::test]
async fn an_approved_code_issues_a_named_key_after_waiting_at_the_announced_interval() {
    let mock = MockServer::start().await;
    let at = serve_device(
        &mock,
        vec![poll_error("authorization_pending"), poll_token()],
    )
    .await;
    let api = api(&mock);
    let started = Instant::now();
    let sign_in = api
        .start_device("MiViA Desktop on LAB".to_owned())
        .await
        .unwrap();
    assert_eq!(sign_in.user_code(), USER_CODE);
    let issued = sign_in.finish(&api).await.unwrap();
    assert_eq!(issued.key.expose_secret(), ISSUED_KEY);

    let at = at.lock().clone();
    assert_eq!(at.len(), 2);
    assert!(at[0] - started >= Duration::from_secs(1));
    assert!(at[1] - at[0] >= Duration::from_secs(1));

    let requests = mock.received_requests().await.unwrap();
    let asked = requests
        .iter()
        .find(|request| request.url.path() == "/api/auth/device/code")
        .unwrap();
    let asked = form(asked);
    assert_eq!(asked["client_id"], "client-1");
    assert_eq!(
        asked["scope"],
        "images:write images:delete jobs:read jobs:write jobs:delete api-keys:issue openid profile email"
    );
    let polled = requests
        .iter()
        .find(|request| request.url.path() == TOKEN_PATH)
        .unwrap();
    let polled = form(polled);
    assert_eq!(
        polled["grant_type"],
        "urn:ietf:params:oauth:grant-type:device_code"
    );
    assert_eq!(polled["client_id"], "client-1");
    assert_eq!(polled["device_code"], "device-1");
    let issue = requests
        .iter()
        .find(|request| request.url.path() == "/api/api-keys/issue")
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&issue.body).unwrap();
    assert_eq!(body, json!({ "name": "MiViA Desktop on LAB" }));
}

#[tokio::test]
async fn slow_down_and_too_many_requests_each_lengthen_the_wait() {
    let mock = MockServer::start().await;
    let at = serve_device(
        &mock,
        vec![
            poll_error("slow_down"),
            ResponseTemplate::new(429),
            poll_token(),
        ],
    )
    .await;
    let api = api(&mock);
    let sign_in = api.start_device(String::new()).await.unwrap();
    sign_in.finish(&api).await.unwrap();
    let at = at.lock().clone();
    assert!(at[1] - at[0] >= Duration::from_secs(6));
    assert!(at[2] - at[1] >= Duration::from_secs(11));
}

#[tokio::test]
async fn a_server_hiccup_while_waiting_does_not_end_the_sign_in() {
    let mock = MockServer::start().await;
    serve_device(&mock, vec![ResponseTemplate::new(503), poll_token()]).await;
    let api = api(&mock);
    let sign_in = api.start_device(String::new()).await.unwrap();
    let issued = sign_in.finish(&api).await.unwrap();
    assert_eq!(issued.key.expose_secret(), ISSUED_KEY);
}

#[tokio::test]
async fn a_denial_or_an_expired_code_ends_the_wait_and_issues_no_key() {
    let cases = [
        ("access_denied", SignInError::Denied),
        ("expired_token", SignInError::Expired),
        ("invalid_grant", SignInError::Expired),
        ("invalid_client", SignInError::Unavailable),
    ];
    for (code, expected) in cases {
        let mock = MockServer::start().await;
        serve_device(&mock, vec![poll_error(code)]).await;
        let api = api(&mock);
        let sign_in = api.start_device(String::new()).await.unwrap();
        assert_eq!(sign_in.finish(&api).await.err(), Some(expected), "{code}");
        assert_eq!(keys_issued(&mock).await, 0, "{code}");
    }
}

#[tokio::test]
async fn a_device_code_that_is_refused_or_unreadable_means_unavailable() {
    let answers = [
        poll_error("invalid_client"),
        ResponseTemplate::new(200).set_body_raw("<html>app</html>", "text/html"),
        ResponseTemplate::new(200)
            .set_body_json(json!({ "device_code": "device-1", "user_code": "" })),
    ];
    for answer in answers {
        let mock = MockServer::start().await;
        serve_client(&mock).await;
        Mock::given(path("/api/auth/device/code"))
            .respond_with(answer)
            .mount(&mock)
            .await;
        let result = api(&mock).start_device(String::new()).await;
        assert_eq!(result.err(), Some(SignInError::Unavailable));
    }
}

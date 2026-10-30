use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use mivia_domain::naming::Language;
use mivia_ports::auth::ServerOrigin;
use mivia_server::{BrowserSignIn, SignInApi, SignInError, key_name, safe_to_open};
use secrecy::ExposeSecret;
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use url::Url;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use super::support::clients;

const CLIENT_PATH: &str = "/api/oauth-clients/by-app/mivia-desktop";
const TOKEN_PATH: &str = "/api/auth/oauth2/token";
const ISSUED_KEY: &str = "mv-issued-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn api(mock: &MockServer) -> SignInApi {
    SignInApi::new(clients(), ServerOrigin(mock.uri()))
}

async fn serve_client(mock: &MockServer) {
    Mock::given(method("GET"))
        .and(path(CLIENT_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "clientId": "client-1" })))
        .mount(mock)
        .await;
}

async fn serve_token_side(mock: &MockServer) {
    serve_client(mock).await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "access_token": "tok-1" })))
        .mount(mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/auth/oauth2/userinfo"))
        .and(header("authorization", "Bearer tok-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "sub": "account-1" })))
        .mount(mock)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/api-keys/issue"))
        .and(header("authorization", "Bearer tok-1"))
        .respond_with(
            ResponseTemplate::new(201).set_body_json(json!({ "id": "key-1", "code": ISSUED_KEY })),
        )
        .mount(mock)
        .await;
}

fn query(url: &str) -> BTreeMap<String, String> {
    Url::parse(url)
        .unwrap()
        .query_pairs()
        .into_owned()
        .collect()
}

struct Return {
    port: u16,
    state: String,
}

impl Return {
    fn of(sign_in: &BrowserSignIn) -> Self {
        let params = query(sign_in.url());
        let redirect = Url::parse(&params["redirect_uri"]).unwrap();
        Self {
            port: redirect.port().unwrap(),
            state: params["state"].clone(),
        }
    }

    fn head(&self, target: &str) -> String {
        format!(
            "GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n",
            self.port
        )
    }

    fn valid(&self) -> String {
        self.head(&format!("/callback?code=abc&state={}", self.state))
    }

    async fn send(&self, raw: &[u8]) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).await.unwrap();
        stream.write_all(raw).await.ok();
        let mut answer = Vec::new();
        stream.read_to_end(&mut answer).await.ok();
        String::from_utf8_lossy(&answer).into_owned()
    }
}

fn pkce_of(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn form(request: &Request) -> BTreeMap<String, String> {
    url::form_urlencoded::parse(&request.body)
        .into_owned()
        .collect()
}

#[tokio::test]
async fn a_valid_return_exchanges_the_code_and_issues_a_named_key() {
    let mock = MockServer::start().await;
    serve_token_side(&mock).await;
    let api = api(&mock);
    let sign_in = api
        .start_browser(Language::En, "MiViA Desktop on LAB".to_owned())
        .await
        .unwrap();
    let url = sign_in.url().to_owned();
    let back = Return::of(&sign_in);
    let finish = tokio::spawn(async move { sign_in.finish(&api).await });
    let answer = back.send(back.valid().as_bytes()).await;
    let issued = finish.await.unwrap().unwrap();

    assert_eq!(issued.key.expose_secret(), ISSUED_KEY);
    assert_eq!(issued.id, "key-1");
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");

    let requests = mock.received_requests().await.unwrap();
    let token = requests
        .iter()
        .find(|request| request.url.path() == TOKEN_PATH)
        .unwrap();
    let sent = form(token);
    let params = query(&url);
    assert_eq!(sent["grant_type"], "authorization_code");
    assert_eq!(sent["client_id"], "client-1");
    assert_eq!(sent["code"], "abc");
    assert_eq!(sent["redirect_uri"], params["redirect_uri"]);
    assert_eq!(pkce_of(&sent["code_verifier"]), params["code_challenge"]);

    let issue = requests
        .iter()
        .find(|request| request.url.path() == "/api/api-keys/issue")
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&issue.body).unwrap();
    assert_eq!(body, json!({ "name": "MiViA Desktop on LAB" }));
}

#[tokio::test]
async fn the_sign_in_url_asks_for_the_agreed_scopes_with_pkce() {
    let mock = MockServer::start().await;
    serve_client(&mock).await;
    let sign_in = api(&mock)
        .start_browser(Language::En, String::new())
        .await
        .unwrap();
    assert!(
        sign_in
            .url()
            .starts_with(&format!("{}/api/auth/oauth2/authorize?", mock.uri()))
    );
    let params = query(sign_in.url());
    assert_eq!(params["response_type"], "code");
    assert_eq!(params["client_id"], "client-1");
    assert_eq!(params["code_challenge_method"], "S256");
    assert_eq!(
        params["scope"],
        "images:write images:delete jobs:read jobs:write jobs:delete api-keys:issue openid profile email"
    );
    let redirect = Url::parse(&params["redirect_uri"]).unwrap();
    assert_eq!(redirect.host_str(), Some("127.0.0.1"));
    assert_eq!(redirect.path(), "/callback");
    assert_ne!(redirect.port(), None);
    assert_eq!(URL_SAFE_NO_PAD.decode(&params["state"]).unwrap().len(), 32);
}

#[tokio::test]
async fn the_client_id_is_asked_for_once_per_sign_in() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(CLIENT_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "clientId": "client-1" })))
        .expect(2)
        .mount(&mock)
        .await;
    let api = api(&mock);
    api.start_browser(Language::En, String::new())
        .await
        .unwrap();
    api.start_browser(Language::En, String::new())
        .await
        .unwrap();
}

#[tokio::test]
async fn only_a_json_answer_with_a_client_id_counts() {
    let answers = [
        ResponseTemplate::new(404).set_body_json(json!({ "clientId": "client-1" })),
        ResponseTemplate::new(200).set_body_raw("<html>app</html>", "text/html"),
        ResponseTemplate::new(200).set_body_json(json!({ "id": "client-1" })),
        ResponseTemplate::new(200).set_body_json(json!({ "clientId": "" })),
        ResponseTemplate::new(302).insert_header("location", "/login"),
        ResponseTemplate::new(500),
    ];
    for answer in answers {
        let mock = MockServer::start().await;
        Mock::given(path(CLIENT_PATH))
            .respond_with(answer)
            .mount(&mock)
            .await;
        let result = api(&mock).start_browser(Language::En, String::new()).await;
        assert!(matches!(result, Err(SignInError::Unavailable)));
    }
}

#[tokio::test]
async fn a_server_that_cannot_be_reached_is_offline() {
    let free = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let closed = format!("http://{}", free.local_addr().unwrap());
    drop(free);
    let result = SignInApi::new(clients(), ServerOrigin(closed))
        .start_browser(Language::En, String::new())
        .await;
    assert!(matches!(result, Err(SignInError::Offline)));
}

#[tokio::test]
async fn requests_that_are_not_the_expected_return_get_an_error_and_do_not_end_the_sign_in() {
    let mock = MockServer::start().await;
    serve_token_side(&mock).await;
    let api = api(&mock);
    let sign_in = api
        .start_browser(Language::En, String::new())
        .await
        .unwrap();
    let back = Return::of(&sign_in);
    let finish = tokio::spawn(async move { sign_in.finish(&api).await });
    let state = &back.state;
    let port = back.port;
    let flipped = if state.starts_with('A') { "B" } else { "A" };
    let bad: Vec<Vec<u8>> = vec![
        back.head(&format!(
            "/callback?code=abc&state={flipped}{}",
            &state[1..]
        ))
        .into_bytes(),
        back.head(&format!("/callback?code=abc&state={state}x"))
            .into_bytes(),
        back.head("/callback?code=abc").into_bytes(),
        back.head(&format!("/callback?state={state}")).into_bytes(),
        back.head(&format!("/other?code=abc&state={state}"))
            .into_bytes(),
        back.head(&format!("//evil/callback?code=abc&state={state}"))
            .into_bytes(),
        back.head("/").into_bytes(),
        back.head("/favicon.ico").into_bytes(),
        format!("POST /callback?code=abc&state={state} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n")
            .into_bytes(),
        format!("GET /callback?code=abc&state={state} HTTP/1.1\r\nHost: evil.example\r\n\r\n")
            .into_bytes(),
        format!("GET /callback?code=abc&state={state} HTTP/1.1\r\nHost: localhost:{port}\r\n\r\n")
            .into_bytes(),
        format!("GET /callback?code=abc&state={state} HTTP/1.1\r\n\r\n").into_bytes(),
        b"not http at all\r\n\r\n".to_vec(),
    ];
    let oversized = format!(
        "GET /callback?code=abc&state={state} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-Pad: {}\r\n\r\n",
        "a".repeat(9 * 1024)
    );
    let answer = back.send(oversized.as_bytes()).await;
    assert!(!answer.contains("200 OK"), "{answer}");
    assert!(!finish.is_finished());
    for raw in &bad {
        let answer = back.send(raw).await;
        assert!(
            answer.starts_with("HTTP/1.1 4"),
            "{} answered {answer}",
            String::from_utf8_lossy(raw)
        );
        assert!(!finish.is_finished());
    }
    let answer = back.send(back.valid().as_bytes()).await;
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    let issued = finish.await.unwrap().unwrap();
    assert_eq!(issued.key.expose_secret(), ISSUED_KEY);
}

#[tokio::test]
async fn a_silent_connection_is_dropped_after_the_read_limit() {
    let mock = MockServer::start().await;
    serve_token_side(&mock).await;
    let api = api(&mock);
    let sign_in = api
        .start_browser(Language::En, String::new())
        .await
        .unwrap();
    let back = Return::of(&sign_in);
    let finish = tokio::spawn(async move { sign_in.finish(&api).await });
    let _silent = TcpStream::connect(("127.0.0.1", back.port)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let started = Instant::now();
    let answer = back.send(back.valid().as_bytes()).await;
    let waited = started.elapsed();
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    assert!(
        (Duration::from_secs(3)..Duration::from_secs(8)).contains(&waited),
        "{waited:?}"
    );
    finish.await.unwrap().unwrap();
}

#[tokio::test]
async fn the_page_is_static_in_the_language_of_the_app() {
    let mock = MockServer::start().await;
    serve_token_side(&mock).await;
    let api = api(&mock);
    let sign_in = api
        .start_browser(Language::De, String::new())
        .await
        .unwrap();
    let back = Return::of(&sign_in);
    let finish = tokio::spawn(async move { sign_in.finish(&api).await });
    let answer = back.send(back.valid().as_bytes()).await;
    finish.await.unwrap().unwrap();
    let lower = answer.to_ascii_lowercase();
    assert!(lower.contains("content-security-policy: default-src 'none'"));
    assert!(lower.contains("referrer-policy: no-referrer"));
    assert!(lower.contains("content-type: text/html; charset=utf-8"));
    assert!(answer.contains("Anmeldung abgeschlossen. Zurück zu MiViA Desktop."));
    assert!(!lower.contains("<script"));
    assert!(!lower.contains("http://"));
    assert!(!lower.contains("https://"));
}

#[tokio::test]
async fn a_return_with_an_error_ends_the_sign_in_with_the_failure_page() {
    let mock = MockServer::start().await;
    serve_token_side(&mock).await;
    let api = api(&mock);
    let sign_in = api
        .start_browser(Language::En, String::new())
        .await
        .unwrap();
    let back = Return::of(&sign_in);
    let finish = tokio::spawn(async move { sign_in.finish(&api).await });
    let target = format!("/callback?error=invalid_scope&state={}", back.state);
    let answer = back.send(back.head(&target).as_bytes()).await;
    assert!(answer.contains("Sign-in failed. Return to MiViA Desktop."));
    assert!(matches!(
        finish.await.unwrap(),
        Err(SignInError::Unavailable)
    ));
}

#[tokio::test]
async fn a_refused_exchange_issues_no_key() {
    let mock = MockServer::start().await;
    serve_client(&mock).await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({ "error": "invalid_grant" })))
        .mount(&mock)
        .await;
    Mock::given(path("/api/api-keys/issue"))
        .respond_with(ResponseTemplate::new(201))
        .expect(0)
        .mount(&mock)
        .await;
    let api = api(&mock);
    let sign_in = api
        .start_browser(Language::En, String::new())
        .await
        .unwrap();
    let back = Return::of(&sign_in);
    let finish = tokio::spawn(async move { sign_in.finish(&api).await });
    back.send(back.valid().as_bytes()).await;
    assert!(matches!(
        finish.await.unwrap(),
        Err(SignInError::Unavailable)
    ));
}

#[test]
fn only_https_or_the_loopback_of_a_test_build_may_be_opened() {
    let cases = [
        (
            "https://app.mivia.ai/api/auth/oauth2/authorize?x=1",
            false,
            true,
        ),
        (
            "https://app.mivia.ai/api/auth/oauth2/authorize?x=1",
            true,
            true,
        ),
        ("http://app.mivia.ai/authorize", false, false),
        ("http://app.mivia.ai/authorize", true, false),
        ("http://127.0.0.1:5000/authorize", false, false),
        ("http://127.0.0.1:5000/authorize", true, true),
        ("http://127.0.0.1.evil.example/authorize", true, false),
        ("http://127.0.0.1@evil.example/authorize", true, false),
        ("file:///C:/Windows/System32/calc.exe", true, false),
        ("calc.exe", true, false),
        ("HTTPS://app.mivia.ai", false, false),
    ];
    for (url, loopback, allowed) in cases {
        assert_eq!(safe_to_open(url, loopback), allowed, "{url} {loopback}");
    }
}

#[test]
fn the_key_is_named_after_the_computer_when_it_has_a_name() {
    assert_eq!(key_name(Some("LAB-PC")), "MiViA Desktop on LAB-PC");
    assert_eq!(key_name(Some("  ")), "MiViA Desktop");
    assert_eq!(key_name(Some("")), "MiViA Desktop");
    assert_eq!(key_name(None), "MiViA Desktop");
    assert_eq!(key_name(Some(&"x".repeat(500))).chars().count(), 128);
}

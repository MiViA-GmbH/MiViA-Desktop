use std::sync::Arc;

use crate::tls::{LEAF, test_ca_verifier, tls_front};
use arc_swap::ArcSwap;
use mivia_domain::ids::AccountId;
use mivia_domain::naming::Language;
use mivia_ports::auth::ServerOrigin;
use mivia_ports::network::{HttpRoute, ProxyEntry};
use mivia_ports::server::{ServerApi, ServerError};
use mivia_server::{
    ClientRoute, HttpClients, HttpServerApi, SignInApi, SignInError, build_clients,
    build_clients_with,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::api_for;

const AGENT: &str = "MiViA-Desktop/2.0.0 (Windows 10.0.26200; stable)";

async fn origin() -> (MockServer, String) {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/version"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"version":"1.0.0"}"#))
        .mount(&mock)
        .await;
    let front = tls_front(*mock.address()).await.unwrap();
    (mock, format!("https://localhost:{}", front.port()))
}

fn api(uri: &str, clients: HttpClients) -> (HttpServerApi, ClientRoute) {
    let shared = Arc::new(ArcSwap::from_pointee(clients));
    let (_, auth) = api_for(uri);
    let api = HttpServerApi::new(auth.origin.clone(), AccountId(1), Arc::clone(&shared), auth);
    (api, ClientRoute(shared))
}

fn trusted() -> HttpClients {
    build_clients_with(AGENT, test_ca_verifier().unwrap()).unwrap()
}

async fn answering_proxy(answer: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let mut buffer = vec![0; 4096];
            let _read = socket.read(&mut buffer).await.unwrap();
            socket.write_all(answer.as_bytes()).await.unwrap();
        }
    });
    address
}

async fn dead_address() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    listener.local_addr().unwrap().to_string()
}

const AUTH_REQUIRED: &str =
    "HTTP/1.1 407 Proxy Authentication Required\r\nProxy-Authenticate: Basic realm=\"x\"\r\n\r\n";
const REFUSED: &str = "HTTP/1.1 403 Forbidden\r\n\r\n";

#[tokio::test]
async fn a_request_after_a_proxy_change_uses_the_new_clients() {
    let (_mock, uri) = origin().await;
    let (api, route) = api(&uri, trusted());
    assert!(api.version().await.is_ok());
    let proxy = answering_proxy(AUTH_REQUIRED).await;
    route.use_entries(&[ProxyEntry::Http(proxy.clone())], false);
    assert_eq!(api.version().await, Err(ServerError::ProxyAuth));
    assert_eq!(route.active(), Some(ProxyEntry::Http(proxy)));
    route.use_entries(&[ProxyEntry::Direct], false);
    assert!(api.version().await.is_ok());
}

#[tokio::test]
async fn a_dead_first_entry_falls_through_to_the_next_one() {
    let (_mock, uri) = origin().await;
    let (api, route) = api(&uri, trusted());
    let dead = ProxyEntry::Http(dead_address().await);
    route.use_entries(&[dead, ProxyEntry::Direct], false);
    assert_eq!(api.version().await, Err(ServerError::Unreachable));
    assert_eq!(route.active(), Some(ProxyEntry::Direct));
    assert!(api.version().await.is_ok());
}

#[tokio::test]
async fn a_refusing_entry_with_a_fallback_is_retried_and_without_one_blocks() {
    let (_mock, uri) = origin().await;
    let (api, route) = api(&uri, trusted());
    let refusing = ProxyEntry::Http(answering_proxy(REFUSED).await);
    route.use_entries(&[refusing.clone(), ProxyEntry::Direct], false);
    assert_eq!(api.version().await, Err(ServerError::Unreachable));
    assert!(api.version().await.is_ok());
    route.use_entries(&[refusing], false);
    assert_eq!(api.version().await, Err(ServerError::ProxyRefused));
}

#[tokio::test]
async fn failures_right_after_a_switch_do_not_skip_the_next_entry() {
    let (_mock, uri) = origin().await;
    let (api, route) = api(&uri, trusted());
    let first = ProxyEntry::Http(dead_address().await);
    let second = ProxyEntry::Http(dead_address().await);
    route.use_entries(&[first, second.clone(), ProxyEntry::Direct], false);
    assert_eq!(api.version().await, Err(ServerError::Unreachable));
    assert_eq!(api.version().await, Err(ServerError::Unreachable));
    assert_eq!(route.active(), Some(second));
}

#[tokio::test]
async fn the_same_entries_keep_the_working_entry_unless_rebuilt_with_new_ones() {
    let (_mock, uri) = origin().await;
    let (api, route) = api(&uri, trusted());
    let dead = ProxyEntry::Http(dead_address().await);
    let entries = [dead.clone(), ProxyEntry::Direct];
    route.use_entries(&entries, false);
    assert_eq!(api.version().await, Err(ServerError::Unreachable));
    route.use_entries(&entries, true);
    assert_eq!(route.active(), Some(ProxyEntry::Direct));
    route.use_entries(
        &[dead.clone(), ProxyEntry::Direct, ProxyEntry::Direct],
        false,
    );
    assert_eq!(route.active(), Some(dead));
}

#[tokio::test]
async fn sign_in_reports_a_proxy_problem_as_offline_like_a_pasted_key() {
    let (_mock, uri) = origin().await;
    let shared = Arc::new(ArcSwap::from_pointee(trusted()));
    let route = ClientRoute(Arc::clone(&shared));
    let api = SignInApi::new(shared, &ServerOrigin(uri));
    for entry in [
        ProxyEntry::Http(answering_proxy(AUTH_REQUIRED).await),
        ProxyEntry::Http(dead_address().await),
    ] {
        route.use_entries(&[entry], false);
        let started = api.start_browser(Language::En, String::new()).await;
        assert_eq!(started.err(), Some(SignInError::Offline));
    }
}

#[tokio::test]
async fn an_untrusted_certificate_blocks_and_is_kept_for_the_it_details() {
    let (_mock, uri) = origin().await;
    let (api, route) = api(&uri, build_clients(AGENT).unwrap());
    assert_eq!(api.version().await, Err(ServerError::TlsUntrusted));
    let seen = route.untrusted().unwrap();
    assert_eq!(seen.der, LEAF);
    assert!(seen.unknown_issuer);
}

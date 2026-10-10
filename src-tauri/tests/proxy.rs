use std::sync::Arc;

use arc_swap::ArcSwap;
use async_trait::async_trait;
use mivia_application::network::{Network, ProxyInputs};
use mivia_domain::ids::AccountId;
use mivia_ports::auth::ServerOrigin;
use mivia_ports::network::{HttpRoute, ProxyEntry, ProxyResolver, SystemProxy};
use mivia_ports::server::{ServerApi, ServerError};
use mivia_server::{ClientRoute, HttpServerApi, build_clients_with};
use mivia_stub::Stub;
use mivia_testkit::auth::FakeAuthorizer;
use mivia_testkit::tls::{test_ca_verifier, tls_front};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const PAC_HOST: &str = "https://app.mivia.ai/";
const KEY: &str = "mvkProxyStubKey0123456789abcdefghijXYZ";

fn proxy(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is set by scripts/proxy-tests.mjs"))
}

async fn origin() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let stub_address = listener.local_addr().unwrap();
    let stub = Stub::new(KEY);
    tokio::spawn(async move { stub.serve(listener).await.unwrap() });
    let front = tls_front(stub_address).await.unwrap();
    format!("https://localhost:{}", front.port())
}

async fn api_through(entries: &[ProxyEntry]) -> (HttpServerApi, ClientRoute, String) {
    let uri = origin().await;
    let clients = build_clients_with(
        "MiViA-Desktop/0.0.0 (Windows test; stable)",
        test_ca_verifier().unwrap(),
    )
    .unwrap();
    let shared = Arc::new(ArcSwap::from_pointee(clients));
    let route = ClientRoute(Arc::clone(&shared));
    route.use_entries(entries, false);
    let auth = Arc::new(FakeAuthorizer::for_origin(AccountId(1), &uri, KEY));
    let api = HttpServerApi::new(ServerOrigin(uri.clone()), AccountId(1), shared, auth);
    (api, route, uri)
}

#[tokio::test]
#[ignore = "needs proxy.py, run by scripts/proxy-tests.mjs"]
async fn a_plain_proxy_carries_the_requests() {
    let (api, _, _) = api_through(&[ProxyEntry::Http(proxy("MIVIA_PROXY_PLAIN"))]).await;
    assert!(api.version().await.is_ok());
}

#[tokio::test]
#[ignore = "needs proxy.py, run by scripts/proxy-tests.mjs"]
async fn a_proxy_asking_for_basic_sign_in_blocks_with_proxy_auth() {
    let (api, _, _) = api_through(&[ProxyEntry::Http(proxy("MIVIA_PROXY_AUTH"))]).await;
    assert_eq!(api.version().await, Err(ServerError::ProxyAuth));
}

#[tokio::test]
#[ignore = "needs proxy.py, run by scripts/proxy-tests.mjs"]
async fn a_proxy_refusing_the_host_blocks_with_proxy_refused() {
    let (api, _, _) = api_through(&[ProxyEntry::Http(proxy("MIVIA_PROXY_REFUSE"))]).await;
    assert_eq!(api.version().await, Err(ServerError::ProxyRefused));
}

struct PacResolver(String);

#[async_trait]
impl ProxyResolver for PacResolver {
    async fn resolve(&self, _loopback: &str) -> Option<SystemProxy> {
        let entries = mivia_os::proxy_for_url(PAC_HOST, false, Some(&self.0))
            .await
            .ok()?;
        Some(SystemProxy {
            entries,
            pac_url: Some(self.0.clone()),
            auto_detect: false,
            machine: false,
        })
    }
}

async fn pac_server(script: String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let mut buffer = vec![0; 4096];
            let _read = socket.read(&mut buffer).await.unwrap();
            let answer = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/x-ns-proxy-autoconfig\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{script}",
                script.len()
            );
            socket.write_all(answer.as_bytes()).await.unwrap();
        }
    });
    format!("http://127.0.0.1:{port}/proxy.pac")
}

#[tokio::test]
#[ignore = "needs proxy.py, run by scripts/proxy-tests.mjs"]
async fn a_pac_file_with_a_dead_first_entry_falls_through_to_the_next() {
    let dead = TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap();
    let plain = proxy("MIVIA_PROXY_PLAIN");
    let script = format!(
        "function FindProxyForURL(url, host) {{ return \"PROXY {dead}; PROXY {plain}\"; }}"
    );
    let pac = pac_server(script).await;
    let (api, route, uri) = api_through(&[ProxyEntry::Direct]).await;
    let network = Network::new(
        Arc::new(PacResolver(pac)),
        Arc::new(route),
        uri,
        ProxyInputs::default(),
    );
    let resolved = network.refresh(false).await;
    assert_eq!(
        resolved.entries,
        [
            ProxyEntry::Http(dead.to_string()),
            ProxyEntry::Http(plain.clone())
        ]
    );
    assert_eq!(api.version().await, Err(ServerError::Unreachable));
    assert_eq!(network.active(), Some(ProxyEntry::Http(plain)));
    assert!(api.version().await.is_ok());
}

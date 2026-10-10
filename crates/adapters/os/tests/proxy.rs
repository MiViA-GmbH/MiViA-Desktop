use mivia_os::{certificate_issuer, proxy_for_url, static_entries};
use mivia_ports::network::ProxyEntry;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const PAC: &str = "function FindProxyForURL(url, host) { \
    if (host == \"direct.example\") return \"DIRECT\"; \
    return \"PROXY 127.0.0.1:1; SOCKS 127.0.0.1:2; PROXY 127.0.0.1:3128; DIRECT\"; }";

async fn pac_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let mut buffer = vec![0; 4096];
            let _read = socket.read(&mut buffer).await.unwrap();
            let answer = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/x-ns-proxy-autoconfig\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{PAC}",
                PAC.len()
            );
            socket.write_all(answer.as_bytes()).await.unwrap();
        }
    });
    format!("http://127.0.0.1:{port}/proxy.pac")
}

fn http(address: &str) -> ProxyEntry {
    ProxyEntry::Http(address.to_owned())
}

#[tokio::test]
async fn a_pac_file_gives_its_entries_in_order_without_socks() {
    let pac = pac_server().await;
    let entries = proxy_for_url("https://app.mivia.ai/", false, Some(&pac)).await;
    assert_eq!(
        entries.unwrap(),
        [
            http("127.0.0.1:1"),
            http("127.0.0.1:3128"),
            ProxyEntry::Direct
        ]
    );
    let direct = proxy_for_url("https://direct.example/", false, Some(&pac)).await;
    assert_eq!(direct.unwrap(), [ProxyEntry::Direct]);
}

#[tokio::test]
async fn an_unreachable_pac_url_is_an_error() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let pac = format!("http://127.0.0.1:{port}/proxy.pac");
    let answer = proxy_for_url("https://app.mivia.ai/", false, Some(&pac)).await;
    assert!(answer.is_err());
}

#[test]
fn static_proxy_strings_pick_the_https_entry_and_honour_the_bypass_list() {
    let url = "https://app.mivia.ai/api";
    let per_protocol = "http=other:1;https=secure:3128";
    assert_eq!(
        static_entries(url, per_protocol, None),
        [http("secure:3128")]
    );
    assert_eq!(
        static_entries(url, "http://plain:8080", None),
        [http("plain:8080")]
    );
    assert_eq!(static_entries(url, "noport", None), [http("noport:80")]);
    assert_eq!(static_entries(url, "ftp=x:1", None), [ProxyEntry::Direct]);
    assert_eq!(
        static_entries(url, "a:1 b:2", None),
        [http("a:1"), http("b:2")]
    );
    let bypass = Some("<local>;*.corp.example;10.*");
    assert_eq!(static_entries(url, "p:1", bypass), [http("p:1")]);
    for host in [
        "https://intranet/",
        "https://files.corp.example/",
        "https://10.0.0.5:443/",
    ] {
        assert_eq!(
            static_entries(host, "p:1", bypass),
            [ProxyEntry::Direct],
            "{host}"
        );
    }
    assert_eq!(
        static_entries(url, "p:1", Some("*.MIVIA.ai")),
        [ProxyEntry::Direct]
    );
}

#[test]
fn the_issuer_name_is_read_from_the_certificate() {
    let leaf = include_bytes!("../../../testing/testkit/tls/leaf.der");
    assert_eq!(
        certificate_issuer(leaf).as_deref(),
        Some("MiViA Test Inspection CA")
    );
    assert_eq!(certificate_issuer(b"not a certificate"), None);
}

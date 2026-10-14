use std::io::Write;

use mivia_stub::Stub;
use mivia_testkit::tls::tls_front;
use tokio::net::TcpListener;

const DEFAULT_KEY: &str = "stub-access-key";

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::io::Result<()> {
    let port = std::env::var("MIVIA_STUB_PORT").unwrap_or_else(|_| "0".to_owned());
    let key = std::env::var("MIVIA_STUB_KEY").unwrap_or_else(|_| DEFAULT_KEY.to_owned());
    let listener = TcpListener::bind(format!("127.0.0.1:{port}")).await?;
    let mut stub = Stub::new(&key);
    let mut address = listener.local_addr()?;
    if std::env::var_os("MIVIA_STUB_TLS").is_some() {
        stub.scheme = "https";
        address = tls_front(address).await?;
    }
    writeln!(std::io::stdout(), "{}://{address}", stub.scheme)?;
    stub.serve(listener).await
}

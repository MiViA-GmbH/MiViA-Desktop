use std::io::Write;

use mivia_stub::Stub;
use tokio::net::TcpListener;

const DEFAULT_KEY: &str = "stub-access-key";

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::io::Result<()> {
    let port = std::env::var("MIVIA_STUB_PORT").unwrap_or_else(|_| "0".to_owned());
    let key = std::env::var("MIVIA_STUB_KEY").unwrap_or_else(|_| DEFAULT_KEY.to_owned());
    let listener = TcpListener::bind(format!("127.0.0.1:{port}")).await?;
    writeln!(std::io::stdout(), "http://{}", listener.local_addr()?)?;
    Stub::new(&key).serve(listener).await
}

use std::net::SocketAddr;
use std::sync::Arc;

use rustls::ServerConfig;
use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::ServerCertVerifier;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;

pub(crate) const CA: &[u8] = include_bytes!("../../../testing/testkit/tls/ca.der");
pub(crate) const LEAF: &[u8] = include_bytes!("../../../testing/testkit/tls/leaf.der");
const LEAF_KEY: &[u8] = include_bytes!("../../../testing/testkit/tls/leaf.key.der");

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

pub(crate) fn test_ca_verifier() -> Result<Arc<dyn ServerCertVerifier>, rustls::Error> {
    let mut roots = rustls::RootCertStore::empty();
    roots.add(CertificateDer::from(CA))?;
    let verifier = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider())
        .build()
        .map_err(|error| rustls::Error::General(error.to_string()))?;
    Ok(verifier)
}

pub(crate) async fn tls_front(target: SocketAddr) -> std::io::Result<SocketAddr> {
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(LEAF_KEY));
    let mut config = ServerConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .and_then(|builder| {
            builder
                .with_no_client_auth()
                .with_single_cert(vec![CertificateDer::from(LEAF)], key)
        })
        .map_err(std::io::Error::other)?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(socket).await else {
                    return;
                };
                if let Ok(mut upstream) = TcpStream::connect(target).await {
                    drop(tokio::io::copy_bidirectional(&mut tls, &mut upstream).await);
                }
            });
        }
    });
    Ok(address)
}

use std::error::Error as StdError;
use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use mivia_ports::network::{HttpRoute, ProxyEntry, UntrustedCertificate};
use mivia_ports::server::ServerError;
use parking_lot::Mutex;
use reqwest::redirect::Policy;
use reqwest::{Client, ClientBuilder, Proxy};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{CertificateError, ClientConfig, DigitallySignedStruct, SignatureScheme};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const API_TIMEOUT: Duration = Duration::from_secs(30);
const POOL_IDLE: Duration = Duration::from_secs(90);
const H2_KEEP_ALIVE: Duration = Duration::from_secs(30);
const MAX_CERTIFICATE_BYTES: usize = 64 * 1024;

pub type BuildError = Box<dyn StdError + Send + Sync>;

#[derive(Debug)]
struct Recorder {
    inner: Arc<dyn ServerCertVerifier>,
    seen: Mutex<Option<UntrustedCertificate>>,
}

impl ServerCertVerifier for Recorder {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let verified = self.inner.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        );
        let seen = match &verified {
            Err(rustls::Error::InvalidCertificate(problem))
                if end_entity.len() <= MAX_CERTIFICATE_BYTES =>
            {
                Some(UntrustedCertificate {
                    der: end_entity.to_vec(),
                    unknown_issuer: matches!(problem, CertificateError::UnknownIssuer),
                })
            }
            Ok(_) | Err(_) => None,
        };
        *self.seen.lock() = seen;
        verified
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

#[derive(Debug)]
struct Setup {
    user_agent: String,
    provider: Arc<CryptoProvider>,
    verifier: Arc<Recorder>,
}

#[derive(Clone, Debug)]
pub struct HttpClients {
    pub api: Client,
    pub upload: Client,
    setup: Arc<Setup>,
    entries: Arc<[ProxyEntry]>,
    index: usize,
    switched: Option<Instant>,
}

fn tls(setup: &Setup, alpn: &[&[u8]]) -> Result<ClientConfig, BuildError> {
    let mut config = ClientConfig::builder_with_provider(Arc::clone(&setup.provider))
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(Arc::clone(&setup.verifier) as _)
        .with_no_client_auth();
    config.alpn_protocols = alpn.iter().map(|protocol| protocol.to_vec()).collect();
    Ok(config)
}

fn base(setup: &Setup, entry: Option<&ProxyEntry>) -> Result<ClientBuilder, BuildError> {
    let builder = Client::builder()
        .user_agent(&setup.user_agent)
        .redirect(Policy::none())
        .connect_timeout(CONNECT_TIMEOUT)
        .no_proxy();
    Ok(match entry {
        Some(ProxyEntry::Http(address)) => builder.proxy(Proxy::all(format!("http://{address}"))?),
        Some(ProxyEntry::Direct) | None => builder,
    })
}

fn build(
    setup: Arc<Setup>,
    entries: Arc<[ProxyEntry]>,
    index: usize,
) -> Result<HttpClients, BuildError> {
    let entry = entries.get(index);
    let api = base(&setup, entry)?
        .tls_backend_preconfigured(tls(&setup, &[b"h2", b"http/1.1"])?)
        .timeout(API_TIMEOUT)
        .pool_idle_timeout(POOL_IDLE)
        .http2_keep_alive_interval(H2_KEEP_ALIVE)
        .http2_keep_alive_while_idle(true)
        .build()?;
    let upload = base(&setup, entry)?
        .tls_backend_preconfigured(tls(&setup, &[b"http/1.1"])?)
        .http1_only()
        .build()?;
    Ok(HttpClients {
        api,
        upload,
        setup,
        entries,
        index,
        switched: None,
    })
}

pub fn build_clients_with(
    user_agent: &str,
    verifier: Arc<dyn ServerCertVerifier>,
) -> Result<HttpClients, BuildError> {
    let setup = Setup {
        user_agent: user_agent.to_owned(),
        provider: Arc::new(rustls::crypto::ring::default_provider()),
        verifier: Arc::new(Recorder {
            inner: verifier,
            seen: Mutex::new(None),
        }),
    };
    build(Arc::new(setup), Arc::from([ProxyEntry::Direct]), 0)
}

pub fn build_clients(user_agent: &str) -> Result<HttpClients, BuildError> {
    drop(rustls::crypto::ring::default_provider().install_default());
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = rustls_platform_verifier::Verifier::new(provider)?;
    build_clients_with(user_agent, Arc::new(verifier))
}

impl HttpClients {
    pub fn route(&self, entries: &[ProxyEntry], rebuild: bool) -> Result<Option<Self>, BuildError> {
        if !rebuild && *self.entries == *entries {
            return Ok(None);
        }
        let same = *self.entries == *entries;
        let index = if same { self.index } else { 0 };
        let entries = if same {
            Arc::clone(&self.entries)
        } else {
            Arc::from(entries)
        };
        let mut next = build(Arc::clone(&self.setup), entries, index)?;
        next.switched = self.switched;
        Ok(Some(next))
    }

    fn advanced(&self, now: Instant) -> Option<Self> {
        let settled = self
            .switched
            .is_none_or(|at| now.duration_since(at) >= CONNECT_TIMEOUT);
        if !settled || self.entries.len() < 2 {
            return None;
        }
        let next = self.index.saturating_add(1) % self.entries.len();
        let mut clients = build(Arc::clone(&self.setup), Arc::clone(&self.entries), next)
            .inspect_err(|error| tracing::warn!(%error, "proxy.advanceFailed"))
            .ok()?;
        clients.switched = Some(now);
        Some(clients)
    }

    fn active(&self) -> Option<ProxyEntry> {
        self.entries.get(self.index).cloned()
    }
}

pub(crate) fn fall_through(clients: &ArcSwap<HttpClients>, error: ServerError) -> ServerError {
    if !matches!(error, ServerError::Unreachable | ServerError::ProxyRefused) {
        return error;
    }
    let current = clients.load_full();
    if let Some(next) = current.advanced(Instant::now()) {
        let previous = clients.compare_and_swap(&current, Arc::new(next));
        if Arc::ptr_eq(&previous, &current) {
            tracing::info!(from = current.index, "proxy.nextEntry");
        }
    }
    if current.index.saturating_add(1) >= current.entries.len() {
        error
    } else {
        ServerError::Unreachable
    }
}

pub struct ClientRoute(pub Arc<ArcSwap<HttpClients>>);

impl HttpRoute for ClientRoute {
    fn use_entries(&self, entries: &[ProxyEntry], rebuild: bool) {
        match self.0.load().route(entries, rebuild) {
            Ok(Some(next)) => self.0.store(Arc::new(next)),
            Ok(None) => {}
            Err(error) => tracing::error!(%error, "http.clientsFailed"),
        }
    }

    fn active(&self) -> Option<ProxyEntry> {
        self.0.load().active()
    }

    fn untrusted(&self) -> Option<UntrustedCertificate> {
        self.0.load().setup.verifier.seen.lock().clone()
    }
}

pub fn user_agent(version: &str, windows: &str, channel: &str) -> String {
    format!("MiViA-Desktop/{version} (Windows {windows}; {channel})")
}

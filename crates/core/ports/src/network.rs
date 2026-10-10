use async_trait::async_trait;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProxyEntry {
    Direct,
    Http(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SystemProxy {
    pub entries: Vec<ProxyEntry>,
    pub pac_url: Option<String>,
    pub auto_detect: bool,
    pub machine: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UntrustedCertificate {
    pub der: Vec<u8>,
    pub unknown_issuer: bool,
}

#[async_trait]
pub trait ProxyResolver: Send + Sync {
    async fn resolve(&self, url: &str) -> Option<SystemProxy>;
}

pub trait HttpRoute: Send + Sync {
    fn use_entries(&self, entries: &[ProxyEntry], rebuild: bool);
    fn active(&self) -> Option<ProxyEntry>;
    fn untrusted(&self) -> Option<UntrustedCertificate>;
}

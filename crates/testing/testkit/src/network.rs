use async_trait::async_trait;
use mivia_ports::network::{
    HttpRoute, ProxyEntry, ProxyResolver, SystemProxy, UntrustedCertificate,
};
use parking_lot::Mutex;

#[derive(Default)]
pub struct FakeResolver {
    pub answer: Mutex<Option<SystemProxy>>,
    pub calls: Mutex<u32>,
}

#[async_trait]
impl ProxyResolver for FakeResolver {
    async fn resolve(&self, _url: &str) -> Option<SystemProxy> {
        *self.calls.lock() += 1;
        self.answer.lock().clone()
    }
}

#[derive(Default)]
pub struct FakeRoute {
    pub applied: Mutex<Vec<(Vec<ProxyEntry>, bool)>>,
    pub untrusted: Mutex<Option<UntrustedCertificate>>,
}

impl HttpRoute for FakeRoute {
    fn use_entries(&self, entries: &[ProxyEntry], rebuild: bool) {
        self.applied.lock().push((entries.to_vec(), rebuild));
    }

    fn active(&self) -> Option<ProxyEntry> {
        self.applied
            .lock()
            .last()
            .and_then(|(entries, _)| entries.first().cloned())
    }

    fn untrusted(&self) -> Option<UntrustedCertificate> {
        self.untrusted.lock().clone()
    }
}

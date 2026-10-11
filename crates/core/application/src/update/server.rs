use mivia_domain::update::{Offer, satisfies, too_old};
use mivia_ports::ipc::update::RequiredUpdate;
use mivia_ports::server::ServerVersion;
use mivia_ports::telemetry::{MinVersionState, UpdateMinVersion};

use super::Updates;
use super::report::telemetry_version;

impl Updates {
    pub async fn connected(&self) {
        let _work = self.work.lock().await;
        if let Some(server) = self.read_server().await {
            let offer = self.inner.lock().offer.clone();
            self.apply_server(&server, offer.as_ref()).await;
        }
    }

    pub(super) async fn read_server(&self) -> Option<ServerVersion> {
        let fresh = self.ports.env.server_version().await;
        let mut inner = self.inner.lock();
        if fresh.is_some() {
            inner.server = fresh;
        }
        inner.server.clone()
    }

    pub(super) async fn apply_server(&self, server: &ServerVersion, offer: Option<&Offer>) -> bool {
        let min = server.min_desktop_version.clone();
        let old = too_old(
            &self.config.current,
            min.as_deref(),
            &server.blocked_versions,
        );
        let signed = offer.is_some_and(|offer| satisfies(&offer.version, min.as_deref()));
        self.required(old.then_some(RequiredUpdate { min, signed }))
            .await;
        old
    }

    async fn required(&self, required: Option<RequiredUpdate>) {
        let held = required.as_ref().is_some_and(|required| required.signed);
        if let Some(RequiredUpdate { min, signed }) = &required {
            let state = if *signed {
                MinVersionState::Blocked
            } else {
                MinVersionState::Warned
            };
            tracing::warn!(min = ?min, signed, "update.required");
            self.ports.telemetry.record(
                UpdateMinVersion {
                    min: min.as_deref().and_then(telemetry_version),
                    state: Some(state),
                }
                .into(),
            );
        }
        self.inner.lock().required = required;
        self.ports.env.hold_too_old(held).await;
    }
}

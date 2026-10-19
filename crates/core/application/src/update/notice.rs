use mivia_domain::ids::Timestamp;
use mivia_domain::update::NOTICE_AFTER_MS;
use mivia_ports::ipc::update::UpdateState;
use mivia_ports::store::StoreError;
use mivia_ports::telemetry::{
    NotificationChannel, NotificationClicked, NotificationKind, NotificationShown,
};
use mivia_ports::update::Delivery;

use crate::policy::AutoUpdate;
use crate::settings::update_global;

use super::{Inner, Updates};

pub(super) fn version(inner: &Inner) -> Option<String> {
    let version = &inner.offer.as_ref()?.version;
    let possible = matches!(inner.state, UpdateState::Available | UpdateState::Ready)
        && inner.settings.failed.as_ref() != Some(version);
    possible.then(|| version.clone())
}

const fn channel(delivery: Delivery) -> Option<NotificationChannel> {
    match delivery {
        Delivery::Os => Some(NotificationChannel::Os),
        Delivery::InApp => Some(NotificationChannel::InApp),
        Delivery::Off => None,
    }
}

impl Updates {
    fn due(&self, inner: &Inner) -> Option<(String, bool)> {
        let version = version(inner)?;
        if inner.required.is_some() || inner.settings.notified.as_ref() == Some(&version) {
            return None;
        }
        let ready = inner.state == UpdateState::Ready;
        let waited = inner.settings.pending.as_ref().is_some_and(|pending| {
            pending.version == version
                && self.now().ms_since(Timestamp(pending.since)) >= NOTICE_AFTER_MS
        });
        let due = match self.config.policy {
            Some(AutoUpdate::Off) => false,
            Some(AutoUpdate::NotifyOnly) => ready,
            Some(AutoUpdate::On) => waited,
            None => waited || !inner.settings.automatic,
        };
        due.then_some((version, ready))
    }

    fn shown(&self, delivery: Delivery) -> bool {
        let Some(channel) = channel(delivery) else {
            return false;
        };
        self.ports.telemetry.record(
            NotificationShown {
                kind: Some(NotificationKind::Update),
                channel: Some(channel),
            }
            .into(),
        );
        true
    }

    pub(super) async fn announce(&self) -> Result<(), StoreError> {
        let (notice, changed, due) = {
            let mut inner = self.inner.lock();
            let notice = version(&inner);
            let changed = inner.announced != notice;
            inner.announced.clone_from(&notice);
            let due = self.due(&inner);
            (notice, changed, due)
        };
        if changed {
            self.ports.notifier.notice_changed(notice.as_deref()).await;
        }
        let pending = self.inner.lock().settings.applied.is_some();
        if !pending && due.is_none() {
            return Ok(());
        }
        let accepts = self.ports.env.activity().await.accepts_notifications;
        self.applied(Some(accepts)).await?;
        let Some((version, ready)) = due.filter(|_| accepts) else {
            return Ok(());
        };
        let delivery = self.ports.notifier.show_available(&version, ready).await;
        if self.shown(delivery) {
            let saved = update_global(self.ports.store.as_ref(), move |global| {
                global.update.notified = Some(version);
            })
            .await?;
            self.inner.lock().settings = saved.update;
        }
        Ok(())
    }

    pub(super) async fn applied(&self, accepts: Option<bool>) -> Result<(), StoreError> {
        let Some(version) = self.inner.lock().settings.applied.clone() else {
            return Ok(());
        };
        let accepts = match accepts {
            Some(accepts) => accepts,
            None => self.ports.env.activity().await.accepts_notifications,
        };
        if !accepts {
            return Ok(());
        }
        let delivery = self.ports.notifier.show_applied(&version).await;
        self.shown(delivery);
        let saved = update_global(self.ports.store.as_ref(), |global| {
            global.update.applied = None;
        })
        .await?;
        self.inner.lock().settings = saved.update;
        Ok(())
    }

    pub async fn mark_seen(&self, clicked: bool) -> Result<(), StoreError> {
        if clicked {
            self.ports.telemetry.record(
                NotificationClicked {
                    kind: Some(NotificationKind::Update),
                    channel: Some(NotificationChannel::InApp),
                }
                .into(),
            );
        }
        let unseen = {
            let inner = self.inner.lock();
            version(&inner).filter(|version| inner.settings.notified.as_ref() != Some(version))
        };
        let Some(version) = unseen else {
            return Ok(());
        };
        let saved = update_global(self.ports.store.as_ref(), move |global| {
            global.update.notified = Some(version);
        })
        .await?;
        self.inner.lock().settings = saved.update;
        Ok(())
    }
}

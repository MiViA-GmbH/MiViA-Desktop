use mivia_domain::ids::Timestamp;
use mivia_domain::update::AUTO_INSTALL_AFTER_MS;
use mivia_ports::ipc::update::UpdateView;
use mivia_ports::store::StoreError;
use mivia_ports::telemetry::{DeferReason, InstallTrigger, UpdateDeferred, UpdateInstallStarted};

use crate::settings::update_global;

use super::Updates;
use super::report::{defer_reason, telemetry_version};
use super::settings::{Pending, PendingInstall};
impl Updates {
    pub async fn tick(&self) -> Result<(), StoreError> {
        self.announce().await?;
        let (ready, forced, pending, failed) = {
            let inner = self.inner.lock();
            (
                inner.ready.clone(),
                inner
                    .required
                    .as_ref()
                    .is_some_and(|required| required.signed),
                inner.settings.pending.clone(),
                inner.settings.failed.clone(),
            )
        };
        let Some(ready) = ready else {
            return Ok(());
        };
        if forced {
            let busy = self.ports.env.activity().await.busy;
            if !busy.contains(&DeferReason::Upload) {
                self.install(InstallTrigger::Forced).await?;
            }
            return Ok(());
        }
        self.install_when_idle(&ready, pending, failed).await
    }

    async fn install_when_idle(
        &self,
        ready: &str,
        pending: Option<Pending>,
        failed: Option<String>,
    ) -> Result<(), StoreError> {
        let due = pending.is_some_and(|pending| {
            self.now().ms_since(Timestamp(pending.since)) >= AUTO_INSTALL_AFTER_MS
        });
        if !due || !self.installs_when_idle() || failed.as_deref() == Some(ready) {
            return Ok(());
        }
        match defer_reason(&self.ports.env.activity().await) {
            Some(reason) => {
                tracing::info!(?reason, "update.deferred");
                self.ports.telemetry.record(
                    UpdateDeferred {
                        reason: Some(reason),
                    }
                    .into(),
                );
                Ok(())
            }
            None => self.install(InstallTrigger::IdleAuto).await.map(drop),
        }
    }

    pub async fn install_now(&self) -> Result<UpdateView, StoreError> {
        let offer = self.inner.lock().offer.clone();
        if let Some(offer) = offer {
            let work = self.work.lock().await;
            self.download(&offer).await;
            drop(work);
            self.install(InstallTrigger::User).await?;
        }
        Ok(self.view())
    }

    pub async fn on_quit(&self) -> Result<bool, StoreError> {
        let (ready, failed) = {
            let inner = self.inner.lock();
            (inner.ready.clone(), inner.settings.failed.clone())
        };
        if ready.is_none() || ready == failed {
            return Ok(false);
        }
        self.install(InstallTrigger::Quit).await
    }

    async fn install(&self, trigger: InstallTrigger) -> Result<bool, StoreError> {
        let Some(to) = self.inner.lock().ready.clone() else {
            return Ok(false);
        };
        let from = self.config.current.clone();
        let record = PendingInstall {
            from: from.clone(),
            to: to.clone(),
            at: self.now().0,
        };
        update_global(self.ports.store.as_ref(), move |global| {
            global.update.pending_install = Some(record);
        })
        .await?;
        tracing::info!(%from, %to, ?trigger, "update.installStarted");
        self.ports.telemetry.record(
            UpdateInstallStarted {
                from: telemetry_version(&from),
                to: telemetry_version(&to),
                trigger: Some(trigger),
            }
            .into(),
        );
        if let Err(error) = self.ports.updater.install(&to).await {
            self.failed(error, Some(&to));
            update_global(self.ports.store.as_ref(), |global| {
                global.update.pending_install = None;
            })
            .await?;
            return Ok(false);
        }
        Ok(true)
    }
}

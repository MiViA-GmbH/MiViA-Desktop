use std::sync::Arc;

use mivia_domain::ids::Timestamp;
use mivia_domain::update::{
    FIRST_CHECK_MS, MANUAL_GAP_MS, Offer, Rules, after_check, next_check, parse, vet,
};
use mivia_ports::clock::Clock;
use mivia_ports::id_gen::IdGen;
use mivia_ports::ipc::update::{
    RequiredUpdate, UpdateChannel, UpdatePolicy, UpdateSettingsInput, UpdateState, UpdateView,
};
use mivia_ports::server::ServerVersion;
use mivia_ports::store::{Store, StoreError};
use mivia_ports::telemetry::{CheckState, TelemetrySink, UpdateDownloaded};
use mivia_ports::time::iso_utc;
use mivia_ports::update::{UpdateEnv, UpdateError, UpdateNotifier, Updater};
use parking_lot::Mutex;

use crate::policy::AutoUpdate;
use crate::settings::update_global;

mod install;
mod notice;
mod report;
mod server;
mod settings;

pub use report::telemetry_version;
pub use settings::{Pending, PendingInstall, UpdateSettings};
use settings::{read, settle_install};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    Start,
    Schedule,
    Manual,
}

pub struct UpdatePorts {
    pub store: Arc<dyn Store>,
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<dyn IdGen>,
    pub telemetry: Arc<dyn TelemetrySink>,
    pub updater: Arc<dyn Updater>,
    pub env: Arc<dyn UpdateEnv>,
    pub notifier: Arc<dyn UpdateNotifier>,
}

pub struct UpdateConfig {
    pub current: String,
    pub policy: Option<AutoUpdate>,
    pub prefix: String,
}

struct Inner {
    settings: UpdateSettings,
    state: UpdateState,
    offer: Option<Offer>,
    ready: Option<String>,
    required: Option<RequiredUpdate>,
    server: Option<ServerVersion>,
    next_check: Timestamp,
    last_manual: Option<Timestamp>,
    announced: Option<String>,
}

pub struct Updates {
    ports: UpdatePorts,
    config: UpdateConfig,
    inner: Mutex<Inner>,
    work: tokio::sync::Mutex<()>,
}

impl Updates {
    pub async fn start(ports: UpdatePorts, config: UpdateConfig) -> Result<Self, StoreError> {
        let now = ports.clock.now();
        let before = read(ports.store.as_ref()).await?.pending_install;
        let current = config.current.clone();
        let settings = update_global(ports.store.as_ref(), move |global| {
            settle_install(&mut global.update, &current);
        })
        .await?
        .update;
        let updates = Self {
            inner: Mutex::new(Inner {
                next_check: next_check(
                    now.plus_ms(FIRST_CHECK_MS),
                    settings.last_check_at.map(Timestamp),
                ),
                settings,
                state: UpdateState::Idle,
                offer: None,
                ready: None,
                required: None,
                server: None,
                last_manual: None,
                announced: None,
            }),
            ports,
            config,
            work: tokio::sync::Mutex::new(()),
        };
        if let Some(pending) = before {
            updates.settled(&pending, now);
        }
        updates.ports.notifier.withdraw();
        updates.applied(None).await?;
        Ok(updates)
    }

    fn now(&self) -> Timestamp {
        self.ports.clock.now()
    }

    fn automatic_checks(&self) -> bool {
        self.config.policy != Some(AutoUpdate::Off)
    }

    fn downloads_without_asking(&self, trigger: Trigger) -> bool {
        match self.config.policy {
            Some(AutoUpdate::On | AutoUpdate::NotifyOnly) => true,
            Some(AutoUpdate::Off) => trigger == Trigger::Manual,
            None => trigger == Trigger::Manual || self.inner.lock().settings.automatic,
        }
    }

    fn installs_when_idle(&self) -> bool {
        match self.config.policy {
            Some(AutoUpdate::On) => true,
            Some(AutoUpdate::NotifyOnly | AutoUpdate::Off) => false,
            None => self.inner.lock().settings.automatic,
        }
    }

    fn channel(&self, stored: UpdateChannel) -> UpdateChannel {
        if parse(&self.config.current).is_some_and(|version| version.is_prerelease()) {
            UpdateChannel::Beta
        } else {
            stored
        }
    }

    pub fn next_check_at(&self) -> Timestamp {
        self.inner.lock().next_check
    }

    pub fn view(&self) -> UpdateView {
        let inner = self.inner.lock();
        UpdateView {
            current: self.config.current.clone(),
            channel: self.channel(inner.settings.channel),
            automatic: inner.settings.automatic,
            policy: self.config.policy.map(|policy| match policy {
                AutoUpdate::On => UpdatePolicy::On,
                AutoUpdate::NotifyOnly => UpdatePolicy::NotifyOnly,
                AutoUpdate::Off => UpdatePolicy::Off,
            }),
            state: inner.state,
            version: inner.offer.as_ref().map(|offer| offer.version.clone()),
            last_check_at: inner
                .settings
                .last_check_at
                .map(|at| iso_utc(Timestamp(at))),
            failed_install: inner.settings.failed.clone(),
            notice: notice::version(&inner).is_some(),
            required: inner.required.clone(),
            progress: (inner.state == UpdateState::Downloading)
                .then(|| self.ports.updater.progress())
                .flatten(),
        }
    }

    pub async fn set_settings(&self, input: UpdateSettingsInput) -> Result<UpdateView, StoreError> {
        let saved = update_global(self.ports.store.as_ref(), move |global| {
            global.update.channel = input.channel;
            global.update.automatic = input.automatic;
        })
        .await?;
        self.inner.lock().settings = saved.update;
        Ok(self.view())
    }

    pub async fn check(&self, trigger: Trigger) -> Result<UpdateView, StoreError> {
        let now = self.now();
        if trigger == Trigger::Manual {
            let limited = {
                let mut inner = self.inner.lock();
                let limited = inner
                    .last_manual
                    .is_some_and(|at| now.ms_since(at) < MANUAL_GAP_MS);
                if !limited {
                    inner.last_manual = Some(now);
                }
                limited
            };
            if limited {
                return Ok(self.view());
            }
        } else if !self.automatic_checks() {
            self.connected().await;
            let random = self.ports.ids.uuid().as_u64_pair().1;
            self.inner.lock().next_check = after_check(now, random);
            return Ok(self.view());
        }
        let work = self.work.lock().await;
        self.run_check(trigger, now).await?;
        drop(work);
        self.tick().await?;
        Ok(self.view())
    }

    async fn run_check(&self, trigger: Trigger, now: Timestamp) -> Result<(), StoreError> {
        let previous = std::mem::replace(&mut self.inner.lock().state, UpdateState::Checking);
        let server = self.read_server().await;
        let blocked = server
            .as_ref()
            .map(|server| server.blocked_versions.clone())
            .unwrap_or_default();
        let channel = self.channel(self.inner.lock().settings.channel).into();
        let rules = Rules {
            channel,
            blocked: &blocked,
            prefix: &self.config.prefix,
        };
        let (check_state, offer) = self.fetch(&rules).await;
        self.record_check(check_state, offer.as_ref(), trigger);
        let old = match &server {
            Some(server) => self.apply_server(server, offer.as_ref()).await,
            None => self.inner.lock().required.is_some(),
        };
        let settled = (check_state == CheckState::Error).then_some(if offer.is_some() {
            previous
        } else {
            UpdateState::Failed
        });
        self.remember(now, offer.clone(), settled).await?;
        if let Some(offer) = offer
            && (old || self.downloads_without_asking(trigger))
        {
            self.download(&offer).await;
        }
        Ok(())
    }

    async fn fetch(&self, rules: &Rules<'_>) -> (CheckState, Option<Offer>) {
        match self.ports.updater.check(rules.channel).await {
            Ok(Some(offer)) => match vet(&offer, &self.config.current, rules) {
                Ok(_) => (CheckState::Found, Some(offer)),
                Err(refusal) => {
                    self.failed(UpdateError::Refused(refusal), None);
                    (CheckState::Error, None)
                }
            },
            Ok(None) => (CheckState::None, None),
            Err(error) => {
                tracing::warn!(?error, "update.checkFailed");
                (CheckState::Error, self.inner.lock().offer.clone())
            }
        }
    }

    async fn remember(
        &self,
        now: Timestamp,
        offer: Option<Offer>,
        settled: Option<UpdateState>,
    ) -> Result<(), StoreError> {
        let found = offer.as_ref().map(|offer| offer.version.clone());
        let saved = update_global(self.ports.store.as_ref(), move |global| {
            global.update.last_check_at = Some(now.0);
            let since = global
                .update
                .pending
                .take()
                .map_or(now.0, |pending| pending.since);
            global.update.pending = found.map(|version| Pending { version, since });
        })
        .await?;
        let random = self.ports.ids.uuid().as_u64_pair().1;
        {
            let mut inner = self.inner.lock();
            inner.settings = saved.update;
            inner.next_check = after_check(now, random);
            if inner.offer != offer {
                inner.ready = None;
            }
            inner.state = match (settled, &offer) {
                (Some(state), _) => state,
                (None, Some(_)) if inner.ready.is_some() => UpdateState::Ready,
                (None, Some(_)) => UpdateState::Available,
                (None, None) => UpdateState::UpToDate,
            };
            inner.offer = offer;
        }
        Ok(())
    }

    async fn download(&self, offer: &Offer) {
        if self.inner.lock().ready.as_deref() == Some(offer.version.as_str()) {
            return;
        }
        self.inner.lock().state = UpdateState::Downloading;
        let started = self.now();
        match self.ports.updater.download(offer).await {
            Ok(bytes) => {
                self.ports.telemetry.record(
                    UpdateDownloaded {
                        to: telemetry_version(&offer.version),
                        bytes: Some(bytes),
                        duration_ms: u64::try_from(self.now().ms_since(started)).ok(),
                    }
                    .into(),
                );
                tracing::info!(to = %offer.version, bytes, "update.downloaded");
                let mut inner = self.inner.lock();
                inner.ready = Some(offer.version.clone());
                inner.state = UpdateState::Ready;
            }
            Err(error) => self.failed(error, Some(&offer.version)),
        }
    }
}

mod events;
mod loops;
mod product;
mod resource;
mod setup;

use std::sync::Arc;

use mivia_application::settings::global_settings;
use mivia_application::setup::SetupDraft;
use mivia_application::telemetry::effective;
use mivia_diagnostics::{Sender, SqliteTelemetrySink, machine_id};
use mivia_os::SystemClock;
use mivia_ports::ipc::setup::SetupOption;
use mivia_ports::server::ServerApi;
use mivia_ports::store::{AccountScope, SettingsScope};
use mivia_ports::telemetry::{ProductEvent, StartState, TelemetrySink};
use parking_lot::Mutex;
use tokio::sync::Notify;

use mivia_application::pipeline::Pipeline;
use mivia_ports::queries::Queries;

use super::Launch;
use super::cx::Services;

use events::{Start, record_start};
pub(crate) use events::{flush_on_stop, record_stop};
use loops::{spawn_sender, spawn_writer};
pub(crate) use product::{
    account_connected, account_disconnected, account_switched, attach_pipeline, custom_report,
    exported, folder_added,
};
pub(crate) use setup::{finished as setup_finished, restarted, step_changed};

const UNKNOWN_MACHINE: &str = "unknown";

#[derive(Clone)]
pub(crate) struct Link {
    pub(crate) api: Arc<dyn ServerApi>,
    pub(crate) scope: AccountScope,
    pub(crate) pipeline: Arc<Pipeline>,
    pub(crate) completed: bool,
}

pub(crate) struct Telemetry {
    pub(crate) sink: Arc<SqliteTelemetrySink>,
    pub(crate) sender: Sender,
    pub(crate) wake: Arc<Notify>,
    machine: Mutex<Option<String>>,
    link: Mutex<Option<Link>>,
}

impl Link {
    pub(crate) fn account(&self) -> String {
        self.scope.0.0.to_string()
    }
}

impl Telemetry {
    pub(crate) fn new(session_id: String, install_id: String) -> Self {
        let wake = Arc::new(Notify::new());
        let waker = Arc::clone(&wake);
        let sink = SqliteTelemetrySink::new(
            Arc::new(SystemClock),
            (session_id, install_id),
            false,
            Box::new(move || waker.notify_one()),
        );
        Self {
            sink: Arc::new(sink),
            sender: Sender::new(),
            wake,
            machine: Mutex::new(None),
            link: Mutex::new(None),
        }
    }

    pub(crate) fn record(&self, event: ProductEvent) {
        self.sink.record(event);
    }

    pub(crate) fn attach(&self, link: Link) {
        self.sink.set_account(Some(link.account()));
        *self.link.lock() = Some(link);
        self.sender.connection_valid();
    }

    pub(crate) fn detach(&self) {
        self.sink.set_account(None);
        *self.link.lock() = None;
    }

    pub(crate) fn linked(&self) -> Option<Link> {
        self.link.lock().clone()
    }

    pub(crate) fn machine_id(&self) -> String {
        let mut cached = self.machine.lock();
        if let Some(id) = cached.as_ref() {
            return id.clone();
        }
        if !self.sink.enabled() {
            return UNKNOWN_MACHINE.to_owned();
        }
        let id = mivia_os::machine_guid()
            .map_or_else(|| UNKNOWN_MACHINE.to_owned(), |guid| machine_id(&guid));
        *cached = Some(id.clone());
        id
    }

    pub(crate) fn shown_machine_id(&self) -> Option<String> {
        if self.sink.enabled() {
            drop(self.machine_id());
        }
        self.machine
            .lock()
            .clone()
            .filter(|id| id != UNKNOWN_MACHINE)
    }
}

pub(super) fn run(services: &Arc<Services>, launch: &Launch, completed: bool) {
    let start = Start {
        launch: if launch.minimized {
            StartState::Autostart
        } else {
            StartState::User
        },
        first_run: !completed,
        unclean: launch.previous_unclean,
        duration_ms: mivia_os::process_age_ms(),
    };
    let started = Arc::clone(services);
    tauri::async_runtime::spawn(async move {
        let route = super::network::begin(&started).await;
        record_start(&started, &start, route.kind());
    });
    spawn_writer(Arc::clone(services));
    drop(spawn_sender(Arc::clone(services)));
}

pub(crate) async fn user_choice(services: &Services) -> bool {
    let completed = global_settings(services.store.as_ref()).await;
    if let Ok(settings) = completed
        && settings.setup_completed
    {
        return settings.telemetry;
    }
    let draft = services
        .store
        .settings(SettingsScope::SetupDraft)
        .await
        .ok()
        .flatten();
    SetupDraft::parse(draft.as_deref())
        .unwrap_or_default()
        .has(SetupOption::Telemetry)
}

pub(crate) async fn refresh_gate(services: &Services) {
    let user = user_choice(services).await;
    let on = effective(services.policies.telemetry, Some(user), None);
    if let Err(error) = services
        .telemetry
        .sink
        .set_enabled(services.store.as_ref(), on)
        .await
    {
        tracing::warn!(?error, "telemetry.gateFailed");
    }
    tracing::info!(enabled = on, "telemetry.gate");
}

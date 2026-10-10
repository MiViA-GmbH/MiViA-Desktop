use std::sync::Arc;
use std::time::Duration;

use mivia_application::intake::{
    Intake, IntakeError, IntakeSink, MarkerSelf, Ports, SIGNAL_CAPACITY, Tick, read_markers, run,
};
use mivia_application::settings::update_global;
use mivia_application::watch_marker::new_marker_id;
use mivia_domain::naming::Language;
use mivia_domain::path::WinPath;
use mivia_os::{NotifyWatcher, OsFileSystem, SystemClock};
use mivia_ports::ipc::folders::FolderNote;
use mivia_ports::store::AccountScope;
use mivia_ports::telemetry::{NoTelemetry, TelemetrySink};
use mivia_store::SqliteStore;
use tauri::async_runtime::JoinHandle;
use tokio::sync::Notify;

use super::cx::{Ready, Services, now};

const START_DELAY: Duration = Duration::from_secs(2);
const START_DELAY_MINIMIZED: Duration = Duration::from_secs(20);

#[derive(Clone, Copy)]
enum Worker {
    Intake,
    Readiness,
    Closer,
}

async fn tick(intake: Arc<Intake>, worker: Worker) -> Result<Tick, IntakeError> {
    match worker {
        Worker::Intake => intake.intake_tick().await,
        Worker::Readiness => intake.readiness_tick().await,
        Worker::Closer => intake.closer_tick().await,
    }
}

fn waker(intake: &Intake, worker: Worker) -> &Notify {
    match worker {
        Worker::Intake => &intake.wakers.intake,
        Worker::Readiness => &intake.wakers.readiness,
        Worker::Closer => &intake.wakers.closer,
    }
}

async fn work(name: &str, intake: Arc<Intake>, worker: Worker) {
    let wake = waker(&intake, worker);
    run(name, &SystemClock, wake, || {
        tick(Arc::clone(&intake), worker)
    })
    .await;
}

fn build(
    store: Arc<SqliteStore>,
    scope: AccountScope,
    version: &str,
    telemetry: Arc<dyn TelemetrySink>,
) -> Intake {
    let (sink, receiver) = IntakeSink::channel(SIGNAL_CAPACITY);
    let ports = Ports {
        store,
        fs: Arc::new(OsFileSystem),
        watcher: Arc::new(NotifyWatcher::new(Arc::clone(&sink) as _)),
        clock: Arc::new(SystemClock),
        telemetry,
    };
    Intake::new(ports, scope, version, (sink, receiver))
}

pub(super) fn compose(store: Arc<SqliteStore>, scope: AccountScope, version: &str) -> Arc<Intake> {
    Arc::new(build(store, scope, version, Arc::new(NoTelemetry)))
}

pub(super) async fn compose_live(services: &Services, scope: AccountScope) -> Arc<Intake> {
    let store = Arc::clone(&services.store);
    let telemetry = Arc::clone(&services.telemetry.sink) as _;
    let intake = build(store, scope, &services.version, telemetry);
    Arc::new(intake.with_marker(marker_self(services).await))
}

pub(super) async fn marker_id(services: &Services) -> Option<String> {
    let mut bytes = [0_u8; 8];
    getrandom::fill(&mut bytes).ok()?;
    let fresh = new_marker_id(u64::from_le_bytes(bytes));
    let settings = update_global(services.store.as_ref(), move |settings| {
        settings.watch_marker_id.get_or_insert(fresh);
    });
    settings.await.ok()?.watch_marker_id
}

pub(super) async fn watched_elsewhere(
    ready: &Ready,
    path: &WinPath,
    language: Language,
) -> Option<FolderNote> {
    if ready.services.policies.disable_watch_marker {
        return None;
    }
    let own = marker_id(&ready.services).await?;
    let dirs = [path.join(language.report_folder_name()).ok()?, path.clone()];
    let at = now();
    for dir in &dirs {
        let mut peers = read_markers(&OsFileSystem, dir).await.into_iter();
        if let Some(peer) = peers.find(|peer| peer.conflicts(&own, path, at)) {
            return Some(FolderNote::WatchedElsewhere {
                machine: peer.machine_name,
                user: peer.windows_user,
            });
        }
    }
    None
}

async fn marker_self(services: &Services) -> Option<MarkerSelf> {
    if services.policies.disable_watch_marker {
        return None;
    }
    Some(MarkerSelf {
        marker_id: marker_id(services).await?,
        machine_name: std::env::var("COMPUTERNAME").unwrap_or_default(),
        windows_user: mivia_os::windows_user().unwrap_or_default(),
    })
}

pub(super) fn start(intake: Arc<Intake>, minimized: bool) -> JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        let delay = if minimized {
            START_DELAY_MINIMIZED
        } else {
            START_DELAY
        };
        tokio::time::sleep(delay).await;
        if let Err(error) = intake.startup().await {
            tracing::error!(?error, "intake.startupFailed");
        }
        super::mark("armed");
        tokio::join!(
            work("readiness-0", Arc::clone(&intake), Worker::Readiness),
            work("readiness-1", Arc::clone(&intake), Worker::Readiness),
            work("closer", Arc::clone(&intake), Worker::Closer),
            work("intake", Arc::clone(&intake), Worker::Intake),
        );
    })
}

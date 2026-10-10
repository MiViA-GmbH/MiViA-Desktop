pub(crate) mod account;
pub(crate) mod backups;
pub(crate) mod cx;

#[cfg(feature = "e2e")]
mod e2e;
mod events;
pub(crate) mod export;
mod feed;
pub(crate) mod folders;
pub(crate) mod history;
mod intake;
pub(crate) mod integration;
pub(crate) mod locale;
pub(crate) mod one_off;
pub(crate) mod open;
mod pipeline;
pub(crate) mod privacy;
pub(crate) mod setup;
mod shutdown;
mod start;
pub(crate) mod support;
pub(crate) mod telemetry;
mod tray;
pub(crate) mod window;

use std::io;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use mivia_application::handoff::Handoff;
use mivia_application::policy::IssueReporting;
use mivia_diagnostics::{
    DiagnosticsContext, Logging, PreviousSession, install_panic_hook, previous_session,
    start_logging, write_marker,
};
use mivia_os::SystemClock;
use mivia_ports::clock::Clock;
use mivia_ports::store::history::Actor;
use mivia_ports::telemetry::{EventKind, ProductEvent};
use mivia_store::{SqliteStore, StoreConfig};
use tauri::{AppHandle, Manager, RunEvent};
use tokio::sync::mpsc::{Receiver, channel};

use crate::instance::{self, Primary};
use cx::Cx;
use shutdown::Shutdown;
use start::{background, search_legacy, services, setup_state};

const HANDOFF_QUEUE: usize = 64;

pub(crate) fn run(primary: Primary, hardening: Option<&io::Result<()>>) -> ExitCode {
    match start(primary, hardening) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "session.startFailed");
            mivia_os::show_error("MiViA Desktop", "MiViA Desktop could not start.");
            ExitCode::FAILURE
        }
    }
}

fn start(
    primary: Primary,
    hardening: Option<&io::Result<()>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(8)
        .thread_keep_alive(Duration::from_secs(5))
        .enable_all()
        .build()?;
    tauri::async_runtime::set(runtime.handle().clone());
    let tauri_context = tauri::generate_context!();
    let version = tauri_context.package_info().version.to_string();
    let logs = primary.data_dir.join("logs");
    let context = DiagnosticsContext::load(&primary.data_dir, &version)?;
    let previous = previous_session(&logs, mivia_os::boot_time_unix_ms(), &version);
    let logging = Arc::new(start_logging(&logs)?);
    install_panic_hook(logs.clone(), context.clone(), Arc::clone(&logging));
    write_marker(&logs, &context)?;
    let previous_unclean = matches!(previous, PreviousSession::Unclean { .. });
    log_start(&context, &primary, previous, hardening);
    let (handoffs, received) = channel(HANDOFF_QUEUE);
    let data_dir = primary.data_dir.clone();
    let minimized = primary.launch.minimized;
    primary.serve(handoffs)?;
    let shutdown = Arc::new(Shutdown::new(runtime, Arc::clone(&logging), logs));
    let watcher = Arc::clone(&shutdown);
    mivia_os::spawn_session_watcher(move || watcher.session_ending())?;
    let launch = Launch {
        data_dir,
        version,
        minimized,
        diagnostics: context,
        logging,
        previous_unclean,
    };
    let app = builder(Arc::clone(&shutdown), launch, received).build(tauri_context)?;
    app.run(move |_, event| on_event(&shutdown, &event));
    Ok(())
}

fn builder(
    shutdown: Arc<Shutdown>,
    launch: Launch,
    received: Receiver<Handoff>,
) -> tauri::Builder<tauri::Wry> {
    let specta = events::builder();
    tauri::Builder::default()
        .invoke_handler(specta.invoke_handler())
        .on_window_event(window::on_event)
        .setup(move |app| {
            specta.mount_events(app);
            app.manage(Cx::new(launch.data_dir.clone()));
            tray::build(app.handle())?;
            mark("tray_ready");
            let inbox = launch.data_dir.clone();
            tauri::async_runtime::spawn(init(app.handle().clone(), shutdown, launch));
            tauri::async_runtime::spawn(receive(app.handle().clone(), received, inbox));
            Ok(())
        })
}

fn on_event(shutdown: &Shutdown, event: &RunEvent) {
    if let RunEvent::ExitRequested {
        code: None, api, ..
    } = event
    {
        api.prevent_exit();
    }
    if matches!(event, RunEvent::Exit) {
        shutdown.finish();
    }
}

fn log_start(
    context: &DiagnosticsContext,
    primary: &Primary,
    previous: PreviousSession,
    hardening: Option<&io::Result<()>>,
) {
    tracing::info!(
        session_id = %context.session_id,
        install_id = %context.install_id,
        machine_id = %context.machine_id,
        version = %context.version,
        dev = tauri::is_dev(),
        minimized = primary.launch.minimized,
        ignored_args = primary.launch.ignored.len(),
        hardened = hardening.map(Result::is_ok),
        "session.start"
    );
    if let PreviousSession::Unclean { started_at_ms } = previous {
        tracing::warn!(started_at_ms, "session.unclean");
    }
    if let Some(Err(error)) = hardening {
        tracing::warn!(%error, "process.hardeningFailed");
    }
}

fn mark(name: &str) {
    tracing::info!(target: "perf", ms_since_process_create = mivia_os::process_age_ms(), "{name}");
}

struct Launch {
    data_dir: PathBuf,
    version: String,
    minimized: bool,
    diagnostics: DiagnosticsContext,
    logging: Arc<Logging>,
    previous_unclean: bool,
}

async fn open_store(launch: &Launch, observer: feed::Feed) -> Option<Arc<SqliteStore>> {
    let path = launch.data_dir.join("mivia.db");
    let opened = SqliteStore::open(&path, StoreConfig::production(), Arc::new(observer)).await;
    let store = opened
        .inspect_err(|error| tracing::error!(?error, "store.openFailed"))
        .ok()?;
    store.set_actor(Actor {
        windows_user: mivia_os::windows_user().unwrap_or_default(),
        tz: SystemClock.time_zone(),
    });
    Some(Arc::new(store))
}

async fn init(app: AppHandle, shutdown: Arc<Shutdown>, launch: Launch) {
    let (observer, commits) = feed::channel();
    tauri::async_runtime::spawn(feed::run(app.clone(), commits));
    let Some(store) = open_store(&launch, observer).await else {
        return;
    };
    shutdown.attach_store(Arc::clone(&store));
    mark("db_ready");
    #[cfg(feature = "e2e")]
    if let Some(seed) = e2e::seed_from_env()
        && let Err(error) = e2e::seed(&store, &seed, &launch.version).await
    {
        tracing::error!(%error, "e2e.seedFailed");
    }
    let Some(services) = services(store, &launch) else {
        return;
    };
    shutdown.attach_services(Arc::clone(&services));
    telemetry::refresh_gate(&services).await;
    let completed = setup_state(&app.state::<Cx>(), &services).await;
    telemetry::run(&services, &launch, completed);
    app.state::<Cx>()
        .set(account::compose(&services, launch.minimized, None).await);
    let reporting = services.policies.issue_reporting != Some(IssueReporting::Off);
    tray::started(&app, reporting);
    one_off::ingest_inbox(&app, &instance::inbox(&launch.data_dir)).await;
    if !completed {
        tray::hide(&app);
        open_at_start(&app);
        search_legacy(&app, &services);
    }
    #[cfg(feature = "e2e")]
    if completed {
        open_at_start(&app);
    }
    tauri::async_runtime::spawn(background(Arc::clone(&services), app.clone(), completed));
}

fn open_at_start(app: &AppHandle) {
    let handle = app.clone();
    if let Err(error) = app.run_on_main_thread(move || window::open(&handle)) {
        tracing::error!(%error, "window.openFailed");
    }
}

async fn record_handoff(app: &AppHandle, kind: &'static str) {
    let ready = app.state::<Cx>().ready().await;
    ready
        .services
        .telemetry
        .record(ProductEvent::new(EventKind::InstanceHandoff).tag("kind", kind));
}

async fn receive(app: AppHandle, mut received: Receiver<Handoff>, data_dir: PathBuf) {
    while let Some(handoff) = received.recv().await {
        match handoff {
            Handoff::Wake => {
                tracing::info!("instance.wake");
                record_handoff(&app, "wake").await;
                open_at_start(&app);
            }
            Handoff::Spool(id) => {
                record_handoff(&app, "spool").await;
                one_off::ingest(&app, &instance::spool_file(&data_dir, id)).await;
            }
        }
    }
}

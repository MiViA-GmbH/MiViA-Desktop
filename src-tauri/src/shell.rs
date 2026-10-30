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
pub(crate) mod network;
pub(crate) mod one_off;
pub(crate) mod open;
mod pipeline;
pub(crate) mod preview;
pub(crate) mod privacy;
pub(crate) mod series_actions;
pub(crate) mod setup;
mod shutdown;
pub(crate) mod sign_in;
mod start;
pub(crate) mod support;
pub(crate) mod telemetry;
mod text;
mod tray;
pub(crate) mod update;
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
use mivia_os::{SystemClock, SystemSignal};
use mivia_ports::clock::Clock;
use mivia_ports::store::history::Actor;
use mivia_ports::telemetry::{HandoffKind, InstanceHandoff};
use mivia_store::{SqliteStore, StoreConfig};
use tauri::{AppHandle, Manager, RunEvent};
use tokio::sync::mpsc::{Receiver, UnboundedReceiver, channel, unbounded_channel};

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

fn watch_session(shutdown: &Arc<Shutdown>) -> io::Result<UnboundedReceiver<SystemSignal>> {
    let watcher = Arc::clone(shutdown);
    let (signal, signals) = unbounded_channel();
    mivia_os::spawn_session_watcher(move |event| match event {
        SystemSignal::SessionEnding => watcher.session_ending(),
        other @ (SystemSignal::Resumed
        | SystemSignal::NetworkChanged
        | SystemSignal::ThemeChanged) => {
            if signal.send(other).is_err() {
                tracing::debug!("system.signalDropped");
            }
        }
    })?;
    Ok(signals)
}

fn start(
    primary: Primary,
    hardening: Option<&io::Result<()>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (handoffs, received) = channel(HANDOFF_QUEUE);
    let data_dir = primary.data_dir.clone();
    let launched = (primary.launch.minimized, primary.launch.ignored.len());
    primary.serve(handoffs)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(8)
        .thread_keep_alive(Duration::from_secs(5))
        .enable_all()
        .build()?;
    tauri::async_runtime::set(runtime.handle().clone());
    let tauri_context = tauri::generate_context!();
    let version = tauri_context.package_info().version.to_string();
    let logs = data_dir.join("logs");
    let context = DiagnosticsContext::load(&data_dir, &version)?;
    let previous = previous_session(&logs, mivia_os::boot_time_unix_ms(), &version);
    let logging = Arc::new(start_logging(&logs)?);
    install_panic_hook(logs.clone(), context.clone(), Arc::clone(&logging));
    write_marker(&logs, &context)?;
    let previous_unclean = matches!(previous, PreviousSession::Unclean { .. });
    log_start(&context, launched, previous, hardening);
    let shutdown = Arc::new(Shutdown::new(runtime, Arc::clone(&logging), logs));
    let signals = watch_session(&shutdown)?;
    let launch = Launch {
        data_dir,
        version,
        minimized: launched.0,
        diagnostics: context,
        logging,
        previous_unclean,
    };
    let app = builder(Arc::clone(&shutdown), launch, (received, signals)).build(tauri_context)?;
    app.run(move |_, event| on_event(&shutdown, &event));
    Ok(())
}

fn builder(
    shutdown: Arc<Shutdown>,
    launch: Launch,
    (received, signals): (Receiver<Handoff>, UnboundedReceiver<SystemSignal>),
) -> tauri::Builder<tauri::Wry> {
    let specta = events::builder();
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
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
            tauri::async_runtime::spawn(network::follow(app.handle().clone(), signals));
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
        if let Err(error) = mivia_os::withdraw_toasts() {
            tracing::debug!(%error, "update.withdrawFailed");
        }
        shutdown.finish();
    }
}

fn log_start(
    context: &DiagnosticsContext,
    (minimized, ignored_args): (bool, usize),
    previous: PreviousSession,
    hardening: Option<&io::Result<()>>,
) {
    tracing::info!(
        session_id = %context.session_id,
        install_id = %context.install_id,
        machine_id = %context.machine_id,
        version = %context.version,
        dev = tauri::is_dev(),
        minimized,
        ignored_args,
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
    e2e::prepare(&app, &store, &launch).await;
    let Some(services) = services(store, &launch) else {
        return;
    };
    shutdown.attach_services(Arc::clone(&services));
    update::start(&app, &services, Arc::clone(&shutdown)).await;
    telemetry::refresh_gate(&services).await;
    let completed = setup_state(&app.state::<Cx>(), &services).await;
    telemetry::run(&services, &launch, completed);
    app.state::<Cx>()
        .set(account::compose(&services, launch.minimized, None).await);
    let reporting = services.policies.issue_reporting != Some(IssueReporting::Off);
    tray::started(
        &app,
        reporting,
        locale::language(services.store.as_ref()).await,
    );
    tray::refresh(&app).await;
    one_off::ingest_inbox(&app, &instance::inbox(&launch.data_dir)).await;
    if !completed {
        tray::hide(&app);
        open_at_start(&app);
        search_legacy(&app, &services);
    }
    #[cfg(feature = "e2e")]
    if completed && e2e::cdp_port().is_some() {
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

async fn record_handoff(app: &AppHandle, kind: HandoffKind) {
    let ready = app.state::<Cx>().ready().await;
    ready
        .services
        .telemetry
        .record(InstanceHandoff { kind: Some(kind) }.into());
}

async fn receive(app: AppHandle, mut received: Receiver<Handoff>, data_dir: PathBuf) {
    while let Some(handoff) = received.recv().await {
        match handoff {
            Handoff::Wake => {
                tracing::info!("instance.wake");
                record_handoff(&app, HandoffKind::Wake).await;
                open_at_start(&app);
            }
            Handoff::Spool(id) => {
                record_handoff(&app, HandoffKind::Spool).await;
                one_off::ingest(&app, &instance::spool_file(&data_dir, id)).await;
            }
        }
    }
}

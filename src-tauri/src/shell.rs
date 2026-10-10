pub(crate) mod account;
pub(crate) mod cx;

#[cfg(feature = "e2e")]
mod e2e;
mod events;
mod feed;
pub(crate) mod folders;
mod intake;
pub(crate) mod integration;
pub(crate) mod locale;
pub(crate) mod open;
mod pipeline;
pub(crate) mod setup;
mod shutdown;
mod start;
mod tray;
pub(crate) mod window;

use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use mivia_application::handoff::Handoff;
use mivia_application::spool::{self, MAX_SPOOL_BYTES};
use mivia_diagnostics::{
    DiagnosticsContext, PreviousSession, install_panic_hook, previous_session, start_logging,
    write_marker,
};
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
    log_start(&context, &primary, previous, hardening);
    let (handoffs, received) = channel(HANDOFF_QUEUE);
    let data_dir = primary.data_dir.clone();
    let minimized = primary.launch.minimized;
    primary.serve(handoffs)?;
    let shutdown = Arc::new(Shutdown::new(runtime, logging, logs));
    let watcher = Arc::clone(&shutdown);
    mivia_os::spawn_session_watcher(move || watcher.session_ending())?;
    let launch = Launch {
        data_dir,
        version,
        minimized,
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
            tauri::async_runtime::spawn(receive(received, inbox));
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
}

async fn init(app: AppHandle, shutdown: Arc<Shutdown>, launch: Launch) {
    let path = launch.data_dir.join("mivia.db");
    let (observer, commits) = feed::channel();
    tauri::async_runtime::spawn(feed::run(app.clone(), commits));
    let opened = SqliteStore::open(&path, StoreConfig::production(), Arc::new(observer)).await;
    let pending = std::fs::read_dir(instance::inbox(&launch.data_dir)).map_or(0, Iterator::count);
    tracing::info!(count = pending, "instance.spoolPending");
    let store = match opened {
        Ok(store) => Arc::new(store),
        Err(error) => {
            tracing::error!(?error, "store.openFailed");
            return;
        }
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
    let completed = setup_state(&app.state::<Cx>(), &services).await;
    app.state::<Cx>()
        .set(account::compose(&services, launch.minimized, None).await);
    tray::started(&app);
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

async fn receive(mut received: Receiver<Handoff>, data_dir: PathBuf) {
    while let Some(handoff) = received.recv().await {
        match handoff {
            Handoff::Wake => tracing::info!("instance.wake"),
            Handoff::Spool(id) => log_spool(&instance::spool_file(&data_dir, id), &id.to_string()),
        }
    }
}

fn log_spool(path: &Path, id: &str) {
    let mut bytes = Vec::new();
    let limit = u64::try_from(MAX_SPOOL_BYTES).unwrap_or(u64::MAX) + 1;
    let read = std::fs::File::open(path).and_then(|file| file.take(limit).read_to_end(&mut bytes));
    match read.map(|_| spool::parse(&bytes)) {
        Ok(Ok(parsed)) => tracing::info!(
            spool_id = id,
            paths = parsed.paths.len(),
            rejected = parsed.rejected,
            non_unicode = parsed.non_unicode,
            "instance.handoff"
        ),
        Ok(Err(error)) => tracing::warn!(spool_id = id, ?error, "instance.spoolRejected"),
        Err(error) => tracing::warn!(spool_id = id, %error, "instance.spoolUnreadable"),
    }
}

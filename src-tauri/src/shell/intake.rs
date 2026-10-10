use std::sync::Arc;
use std::time::Duration;

use mivia_application::intake::{
    Intake, IntakeError, IntakeSink, Ports, SIGNAL_CAPACITY, Tick, run,
};
use mivia_os::{NotifyWatcher, OsFileSystem, SystemClock};
use mivia_ports::store::AccountScope;
use mivia_store::SqliteStore;
use tokio::sync::Notify;

const READINESS_WORKERS: usize = 2;
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

fn spawn_worker(name: String, intake: &Arc<Intake>, worker: Worker) {
    let intake = Arc::clone(intake);
    tauri::async_runtime::spawn(async move {
        let wake = waker(&intake, worker);
        run(&name, &SystemClock, wake, || {
            tick(Arc::clone(&intake), worker)
        })
        .await;
    });
}

pub(super) fn compose(store: Arc<SqliteStore>, scope: AccountScope, version: &str) -> Arc<Intake> {
    let (sink, receiver) = IntakeSink::channel(SIGNAL_CAPACITY);
    let ports = Ports {
        store,
        fs: Arc::new(OsFileSystem),
        watcher: Arc::new(NotifyWatcher::new(Arc::clone(&sink) as _)),
        clock: Arc::new(SystemClock),
    };
    Arc::new(Intake::new(ports, scope, version, (sink, receiver)))
}

pub(super) async fn start(intake: Arc<Intake>, minimized: bool) {
    let delay = if minimized {
        START_DELAY_MINIMIZED
    } else {
        START_DELAY
    };
    tokio::time::sleep(delay).await;
    if let Err(error) = intake.startup().await {
        tracing::error!(?error, "intake.startupFailed");
    }
    for index in 0..READINESS_WORKERS {
        spawn_worker(format!("readiness-{index}"), &intake, Worker::Readiness);
    }
    spawn_worker("closer".to_owned(), &intake, Worker::Closer);
    spawn_worker("intake".to_owned(), &intake, Worker::Intake);
    super::mark("armed");
}

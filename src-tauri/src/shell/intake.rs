use std::sync::Arc;
use std::time::Duration;

use mivia_application::intake::{
    Intake, IntakeError, IntakeSink, Ports, SIGNAL_CAPACITY, Tick, run,
};
use mivia_os::{NotifyWatcher, OsFileSystem, SystemClock};
use mivia_ports::store::AccountScope;
use mivia_store::SqliteStore;
use tauri::async_runtime::JoinHandle;
use tokio::sync::Notify;

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

mod closer;
mod folders;
mod plan;
mod readiness;
mod restart;
mod scan;
mod signals;
mod sink;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use mivia_domain::ids::{FolderId, MINUTE_MS, SourceFileId, Timestamp};
use mivia_ports::clock::Clock;
use mivia_ports::fs::{FileSystem, FsError};
use mivia_ports::store::{AccountScope, Store, StoreError, StoreExt, TxError, WriteTx};
use mivia_ports::watch::{FolderWatcher, FsSignal};
use parking_lot::Mutex;
use tokio::sync::Notify;
use tokio::sync::mpsc::Receiver;

pub use crate::worker::{Tick, run};
pub use folders::{AddFolder, FolderProblem, Resumed};
pub use sink::{IntakeSink, SIGNAL_CAPACITY};

pub const LOCAL_RECONCILE_MS: i64 = 10 * MINUTE_MS;
pub const NETWORK_RECONCILE_MS: i64 = 2 * MINUTE_MS;
const SIGNAL_CHUNK: usize = 512;
const PROBE_CHUNK: u32 = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IntakeError {
    Store(StoreError),
    Fs(FsError),
    Folder(FolderProblem),
}

impl From<StoreError> for IntakeError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

pub(super) fn tolerate(result: Result<(), TxError>) -> Result<(), TxError> {
    match result {
        Err(TxError::Illegal(_) | TxError::Conflict) => Ok(()),
        other => other,
    }
}

pub struct Ports {
    pub store: Arc<dyn Store>,
    pub fs: Arc<dyn FileSystem>,
    pub watcher: Arc<dyn FolderWatcher>,
    pub clock: Arc<dyn Clock>,
}

#[derive(Default)]
pub struct Wakers {
    pub intake: Notify,
    pub readiness: Notify,
    pub closer: Notify,
}

#[derive(Default)]
struct Schedule {
    rename_from: Option<FsSignal>,
    due: BTreeMap<FolderId, Timestamp>,
}

pub struct Intake {
    ports: Ports,
    scope: AccountScope,
    app_version: String,
    sink: Arc<IntakeSink>,
    signals: tokio::sync::Mutex<Receiver<FsSignal>>,
    schedule: Mutex<Schedule>,
    in_flight: Arc<Mutex<BTreeSet<SourceFileId>>>,
    serial: tokio::sync::Mutex<()>,
    pub wakers: Arc<Wakers>,
}

impl Intake {
    pub fn new(
        ports: Ports,
        scope: AccountScope,
        app_version: &str,
        signals: (Arc<IntakeSink>, Receiver<FsSignal>),
    ) -> Self {
        let wakers = Arc::clone(signals.0.wakers());
        Self {
            ports,
            scope,
            app_version: app_version.to_owned(),
            sink: signals.0,
            signals: tokio::sync::Mutex::new(signals.1),
            schedule: Mutex::new(Schedule::default()),
            in_flight: Arc::new(Mutex::new(BTreeSet::new())),
            serial: tokio::sync::Mutex::new(()),
            wakers,
        }
    }

    pub fn high_water(&self) -> usize {
        self.sink.high_water()
    }

    fn now(&self) -> Timestamp {
        self.ports.clock.now()
    }

    async fn write<R, F>(&self, work: F) -> Result<R, IntakeError>
    where
        R: Send + 'static,
        F: FnOnce(&mut dyn WriteTx, AccountScope) -> Result<R, TxError> + Send + 'static,
    {
        let scope = self.scope;
        Ok(self.ports.store.write(move |tx| work(tx, scope)).await?)
    }

    fn wake_all(&self) {
        self.wakers.readiness.notify_one();
        self.wakers.closer.notify_one();
    }
}

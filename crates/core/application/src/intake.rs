mod closer;
mod folders;
mod marker;
mod plan;
mod reach;
mod readiness;
mod restart;
mod scan;
mod signals;
mod sink;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use mivia_domain::ids::{FolderId, MINUTE_MS, SourceFileId, Timestamp};
use mivia_domain::intake::{UPLOAD_CAP_BYTES, upload_cap_bytes};
use mivia_ports::clock::Clock;
use mivia_ports::fs::{FileSystem, FsError};
use mivia_ports::store::{AccountScope, Store, StoreError, StoreExt, TxError, WriteTx};
use mivia_ports::telemetry::TelemetrySink;
use mivia_ports::watch::{FolderWatcher, FsSignal};
use parking_lot::Mutex;
use tokio::sync::Notify;
use tokio::sync::mpsc::Receiver;

pub use crate::worker::{Tick, run};
pub use closer::close_all_series;
pub use folders::{AddFolder, FolderProblem, PreparedFolder, Resumed, insert_folder};
pub use marker::{MarkerSelf, read_markers};
pub use reach::{MISSING_AFTER_MS, RETRY_MS};
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
    pub telemetry: Arc<dyn TelemetrySink>,
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
    waits: BTreeMap<FolderId, reach::Wait>,
    confirming: BTreeSet<FolderId>,
    markers: BTreeMap<FolderId, Timestamp>,
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
    marker: Option<MarkerSelf>,
    upload_cap: AtomicU64,
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
            marker: None,
            upload_cap: AtomicU64::new(UPLOAD_CAP_BYTES),
            wakers,
        }
    }

    #[must_use]
    pub fn with_marker(mut self, marker: Option<MarkerSelf>) -> Self {
        self.marker = marker;
        self
    }

    pub fn set_upload_cap(&self, max_upload_size_mb: Option<u32>) {
        let cap = upload_cap_bytes(max_upload_size_mb);
        self.upload_cap.store(cap, Ordering::Relaxed);
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

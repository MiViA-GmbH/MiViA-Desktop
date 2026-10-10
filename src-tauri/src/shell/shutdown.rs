use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, mpsc};
use std::time::Duration;

use mivia_diagnostics::{Logging, remove_marker};
use mivia_store::SqliteStore;
use parking_lot::Mutex;
use tokio::runtime::{Handle, Runtime};

const SESSION_END_CAP: Duration = Duration::from_secs(3);
const QUIT_GRACE: Duration = Duration::from_secs(5);
const RUNTIME_SHUTDOWN: Duration = Duration::from_secs(2);

pub(super) struct Shutdown {
    runtime: Mutex<Option<Runtime>>,
    handle: Handle,
    store: OnceLock<Arc<SqliteStore>>,
    logging: Arc<Logging>,
    logs_dir: PathBuf,
    session_ending: AtomicBool,
    routine: Mutex<Option<mpsc::Receiver<()>>>,
}

impl Shutdown {
    pub(super) fn new(runtime: Runtime, logging: Arc<Logging>, logs_dir: PathBuf) -> Self {
        Self {
            handle: runtime.handle().clone(),
            runtime: Mutex::new(Some(runtime)),
            store: OnceLock::new(),
            logging,
            logs_dir,
            session_ending: AtomicBool::new(false),
            routine: Mutex::new(None),
        }
    }

    pub(super) fn attach_store(&self, store: Arc<SqliteStore>) {
        drop(self.store.set(store));
    }

    pub(super) fn session_ending(&self) {
        self.session_ending.store(true, Ordering::SeqCst);
        self.begin();
    }

    fn begin(&self) {
        let mut routine = self.routine.lock();
        if routine.is_some() {
            return;
        }
        let (done, finished) = mpsc::channel();
        *routine = Some(finished);
        let store = self.store.get().cloned();
        self.handle.spawn(async move {
            if let Some(store) = store
                && let Err(error) = store.checkpoint().await
            {
                tracing::warn!(?error, "store.checkpointFailed");
            }
            done.send(()).ok();
        });
    }

    pub(super) fn finish(&self) {
        let session_end = self.session_ending.load(Ordering::SeqCst);
        let cap = if session_end {
            SESSION_END_CAP
        } else {
            QUIT_GRACE
        };
        self.begin();
        let completed = self
            .routine
            .lock()
            .take()
            .is_some_and(|finished| finished.recv_timeout(cap).is_ok());
        tracing::info!(session_end, completed, "session.stop");
        remove_marker(&self.logs_dir);
        self.logging.flush();
        if let Some(runtime) = self.runtime.lock().take() {
            runtime.shutdown_timeout(RUNTIME_SHUTDOWN);
        }
    }
}

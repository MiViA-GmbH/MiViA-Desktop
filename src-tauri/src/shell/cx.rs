use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use arc_swap::ArcSwap;
use mivia_application::intake::Intake;
use mivia_application::pipeline::Pipeline;
use mivia_application::policy::Policies;
use mivia_application::report_offer::OfferGate;
use mivia_diagnostics::{DiagnosticsContext, Logging};
use mivia_domain::ids::{AccountId, Timestamp};
use mivia_os::SystemClock;
use mivia_ports::auth::{CredentialStore, ServerOrigin};
use mivia_ports::clock::Clock;
use mivia_ports::ipc::error::{AppError, AppErrorKind};
use mivia_ports::ipc::support::PolicyRow;
use mivia_ports::store::AccountScope;
use mivia_server::{ConnectionAuthorizer, HttpClients};
use mivia_store::SqliteStore;
use parking_lot::{Mutex, RwLock};
use tauri::async_runtime::JoinHandle;
use tokio::sync::Notify;

use super::setup::Session;
use super::telemetry::Telemetry;

pub(crate) struct Services {
    pub(crate) store: Arc<SqliteStore>,
    pub(crate) credentials: Arc<dyn CredentialStore>,
    pub(crate) clients: Arc<ArcSwap<HttpClients>>,
    pub(crate) origin: ServerOrigin,
    pub(crate) policies: Policies,
    pub(crate) policy_rows: Vec<PolicyRow>,
    pub(crate) data_dir: PathBuf,
    pub(crate) version: String,
    pub(crate) diagnostics: DiagnosticsContext,
    pub(crate) logging: Arc<Logging>,
    pub(crate) telemetry: Arc<Telemetry>,
}

pub(crate) struct Ready {
    pub(crate) services: Arc<Services>,
    pub(crate) store: Arc<SqliteStore>,
    pub(crate) scope: AccountScope,
    pub(crate) intake: Option<Arc<Intake>>,
    pub(crate) pipeline: Option<Arc<Pipeline>>,
    pub(crate) authorizer: Option<Arc<ConnectionAuthorizer>>,
    pub(crate) tasks: Mutex<Vec<JoinHandle<()>>>,
}

impl Drop for Ready {
    fn drop(&mut self) {
        for task in self.tasks.get_mut().drain(..) {
            task.abort();
        }
    }
}

impl Ready {
    pub(crate) fn without_account(services: Arc<Services>) -> Self {
        Self {
            store: Arc::clone(&services.store),
            services,
            scope: AccountScope(AccountId(0)),
            intake: None,
            pipeline: None,
            authorizer: None,
            tasks: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn intake(&self) -> Result<&Intake, AppError> {
        self.intake.as_deref().ok_or_else(|| no_account("intake"))
    }

    pub(crate) fn pipeline(&self) -> Result<&Pipeline, AppError> {
        self.pipeline
            .as_deref()
            .ok_or_else(|| no_account("pipeline"))
    }

    pub(crate) fn wake(&self) {
        if let Some(pipeline) = &self.pipeline {
            pipeline.wakers.all();
        }
    }

    pub(crate) fn track(&self, task: JoinHandle<()>) {
        self.tasks.lock().push(task);
    }
}

fn no_account(site: &str) -> AppError {
    AppError::new(
        AppErrorKind::NotFound,
        Some("noAccount"),
        "no active account".to_owned(),
        site,
    )
}

pub(crate) fn invalid_id(site: &str) -> AppError {
    AppError::new(
        AppErrorKind::Validation,
        Some("invalidId"),
        "id is not a number".to_owned(),
        site,
    )
}

pub(crate) struct Cx {
    pub(crate) data_dir: PathBuf,
    pub(crate) setup: Session,
    pub(crate) setup_completed: AtomicBool,
    pub(crate) offers: Mutex<OfferGate>,
    pub(crate) report_draft: Mutex<Option<super::support::Draft>>,
    ready: RwLock<Option<Arc<Ready>>>,
    set: Notify,
}

impl Cx {
    pub(crate) fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            setup: Session::default(),
            setup_completed: AtomicBool::new(false),
            offers: Mutex::new(OfferGate::default()),
            report_draft: Mutex::new(None),
            ready: RwLock::new(None),
            set: Notify::new(),
        }
    }

    pub(crate) fn set(&self, ready: Ready) {
        let previous = self.ready.write().replace(Arc::new(ready));
        drop(previous);
        self.set.notify_waiters();
    }

    pub(crate) async fn ready(&self) -> Arc<Ready> {
        loop {
            let notified = self.set.notified();
            if let Some(ready) = self.get() {
                return ready;
            }
            notified.await;
        }
    }

    pub(crate) fn get(&self) -> Option<Arc<Ready>> {
        self.ready.read().clone()
    }

    pub(crate) fn completed(&self) -> bool {
        self.setup_completed.load(Ordering::SeqCst)
    }
}

pub(crate) fn parse_id(text: &str, site: &str) -> Result<i64, AppError> {
    text.parse().map_err(|_| invalid_id(site))
}

pub(crate) fn now() -> Timestamp {
    SystemClock.now()
}

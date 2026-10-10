use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use mivia_application::intake::Intake;
use mivia_application::pipeline::Pipeline;
use mivia_domain::ids::{AccountId, Timestamp};
use mivia_os::SystemClock;
use mivia_ports::clock::Clock;
use mivia_ports::ipc::error::{AppError, AppErrorKind};
use mivia_ports::store::AccountScope;
use mivia_store::SqliteStore;
use tokio::sync::Notify;

pub(crate) struct Ready {
    pub(crate) store: Arc<SqliteStore>,
    pub(crate) scope: AccountScope,
    pub(crate) intake: Option<Arc<Intake>>,
    pub(crate) pipeline: Option<Arc<Pipeline>>,
}

impl Ready {
    pub(crate) fn without_account(store: Arc<SqliteStore>) -> Self {
        Self {
            store,
            scope: AccountScope(AccountId(0)),
            intake: None,
            pipeline: None,
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
    ready: OnceLock<Ready>,
    set: Notify,
}

impl Cx {
    pub(crate) fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            ready: OnceLock::new(),
            set: Notify::new(),
        }
    }

    pub(crate) fn set(&self, ready: Ready) {
        drop(self.ready.set(ready));
        self.set.notify_waiters();
    }

    pub(crate) async fn ready(&self) -> &Ready {
        loop {
            let notified = self.set.notified();
            if let Some(ready) = self.ready.get() {
                return ready;
            }
            notified.await;
        }
    }

    pub(crate) fn get(&self) -> Option<&Ready> {
        self.ready.get()
    }
}

pub(crate) fn parse_id(text: &str, site: &str) -> Result<i64, AppError> {
    text.parse().map_err(|_| invalid_id(site))
}

pub(crate) fn now() -> Timestamp {
    SystemClock.now()
}

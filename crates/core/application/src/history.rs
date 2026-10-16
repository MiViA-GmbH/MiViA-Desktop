mod custom;
mod delete;
mod export;
mod moves;

use std::sync::Arc;

use mivia_domain::ids::{BatchId, ReportId, Timestamp};
use mivia_ports::fs::{FileSystem, FsError};
use mivia_ports::queries::Queries;
use mivia_ports::store::{AccountScope, Store, StoreError, StoreExt, TxError, WriteTx};

pub use custom::CustomRequest;
pub use delete::DeleteOutcome;
pub use export::ExportJob;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryError {
    Store(StoreError),
    Fs(FsError),
    NameRequired,
    NoImages,
    TooMany,
    StillAnalyzing,
    NotReplaceable(Vec<String>),
    NotMoved,
    ChangedAfterMove,
}

impl From<StoreError> for HistoryError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::Tx(TxError::Conflict) => Self::StillAnalyzing,
            other @ (StoreError::Tx(_)
            | StoreError::Panicked
            | StoreError::Unavailable
            | StoreError::DiskFull
            | StoreError::Sql(_)) => Self::Store(other),
        }
    }
}

impl From<FsError> for HistoryError {
    fn from(error: FsError) -> Self {
        Self::Fs(error)
    }
}

pub struct History {
    pub fs: Arc<dyn FileSystem>,
    pub store: Arc<dyn Store>,
    pub queries: Arc<dyn Queries>,
    pub scope: AccountScope,
}

pub fn rename_series(
    scope: AccountScope,
    batch: BatchId,
    label: String,
    now: Timestamp,
) -> impl FnOnce(&mut dyn WriteTx) -> Result<(), TxError> + Send + 'static {
    move |tx| tx.history(scope).rename(batch, &label, now)
}

impl History {
    pub async fn update_report(
        &self,
        report: ReportId,
        now: Timestamp,
    ) -> Result<ReportId, HistoryError> {
        let scope = self.scope;
        Ok(self
            .store
            .write(move |tx| tx.history(scope).update_stale(report, now))
            .await?)
    }

    pub async fn keep_report(&self, report: ReportId, now: Timestamp) -> Result<(), HistoryError> {
        let scope = self.scope;
        self.store
            .write(move |tx| tx.history(scope).keep_as_is(report, now))
            .await?;
        Ok(())
    }
}

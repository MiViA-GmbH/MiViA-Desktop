pub mod history;
pub mod intake;
pub mod pipeline;
pub mod reports;
mod repos;
pub mod telemetry;

use std::any::Any;

use async_trait::async_trait;
use mivia_domain::ids::{AccountId, Timestamp};
use mivia_domain::transition::IllegalTransition;

use crate::ipc::change::Change;
use history::HistoryRepo;
use intake::IntakeRepo;
use pipeline::PipelineRepo;
use telemetry::TelemetryRepo;

pub use reports::{NewReport, ReportRepo};
pub use repos::{
    AccountRepo, AttentionCleared, BatchModelChoice, BatchRepo, ComputationRepo, FolderRepo,
    HoldRepo, NewAccount, NewBatch, NewComputation, NewFolder, NewSourceFile, Origin, SourceClaim,
    SourceFileRepo,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AccountScope(pub AccountId);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TxError {
    NotFound,
    Illegal(IllegalTransition),
    Conflict,
    Corrupt(String),
    DiskFull,
    Sql(String),
}

impl From<IllegalTransition> for TxError {
    fn from(error: IllegalTransition) -> Self {
        Self::Illegal(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreError {
    Tx(TxError),
    Panicked,
    Unavailable,
    DiskFull,
    Sql(String),
}

pub type Erased = Box<dyn Any + Send>;
pub type TxWork = Box<dyn FnOnce(&mut dyn WriteTx) -> Result<Erased, TxError> + Send>;

pub trait WriteTx {
    fn changed(&mut self, change: Change);
    fn accounts(&mut self) -> &mut dyn AccountRepo;
    fn holds(&mut self, scope: AccountScope) -> &mut dyn HoldRepo;
    fn folders(&mut self, scope: AccountScope) -> &mut dyn FolderRepo;
    fn batches(&mut self, scope: AccountScope) -> &mut dyn BatchRepo;
    fn source_files(&mut self, scope: AccountScope) -> &mut dyn SourceFileRepo;
    fn computations(&mut self, scope: AccountScope) -> &mut dyn ComputationRepo;
    fn reports(&mut self, scope: AccountScope) -> &mut dyn ReportRepo;
    fn intake(&mut self, scope: AccountScope) -> &mut dyn IntakeRepo;
    fn pipeline(&mut self, scope: AccountScope) -> &mut dyn PipelineRepo;
    fn history(&mut self, scope: AccountScope) -> &mut dyn HistoryRepo;
    fn telemetry(&mut self) -> &mut dyn TelemetryRepo;
    fn settings(&mut self, scope: SettingsScope) -> Result<Option<String>, TxError>;
    fn set_settings(
        &mut self,
        scope: SettingsScope,
        version: u32,
        json: &str,
    ) -> Result<(), TxError>;
    fn clear_settings(&mut self, scope: SettingsScope) -> Result<(), TxError>;
    fn pending_one_off(&mut self, scope: AccountScope) -> Result<Option<SentImages>, TxError>;
    fn set_pending_one_off(
        &mut self,
        scope: AccountScope,
        pending: Option<&SentImages>,
        now: Timestamp,
    ) -> Result<(), TxError>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SentImages {
    pub spool_ids: Vec<String>,
    pub paths: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsScope {
    Global,
    SetupDraft,
}

impl SettingsScope {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::SetupDraft => "setup_draft",
        }
    }
}

#[async_trait]
pub trait Store: Send + Sync {
    async fn write_erased(&self, work: TxWork) -> Result<Erased, StoreError>;
}

#[async_trait]
pub trait StoreExt: Store {
    async fn write<R, F>(&self, work: F) -> Result<R, StoreError>
    where
        R: Send + 'static,
        F: FnOnce(&mut dyn WriteTx) -> Result<R, TxError> + Send + 'static;
}

#[async_trait]
impl<S: Store + ?Sized> StoreExt for S {
    async fn write<R, F>(&self, work: F) -> Result<R, StoreError>
    where
        R: Send + 'static,
        F: FnOnce(&mut dyn WriteTx) -> Result<R, TxError> + Send + 'static,
    {
        let erased = self
            .write_erased(Box::new(move |tx| {
                work(tx).map(|value| Box::new(value) as Erased)
            }))
            .await?;
        erased
            .downcast::<R>()
            .map(|value| *value)
            .map_err(|_| StoreError::Sql("unexpected result type".to_owned()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Committed {
    pub revision: i64,
    pub changes: Vec<Change>,
}

pub trait CommitObserver: Send + Sync {
    fn committed(&self, committed: &Committed);
}

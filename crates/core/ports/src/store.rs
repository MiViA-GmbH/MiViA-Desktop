pub mod intake;
pub mod pipeline;
pub mod reports;
mod repos;

use std::any::Any;

use async_trait::async_trait;
use mivia_domain::ids::AccountId;
use mivia_domain::transition::IllegalTransition;

use crate::ipc::change::Change;
use intake::IntakeRepo;
use pipeline::PipelineRepo;

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
    fn set_global_settings(&mut self, version: u32, json: &str) -> Result<(), TxError>;
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

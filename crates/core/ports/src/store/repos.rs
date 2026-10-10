use mivia_domain::batch::ClosureReason;
use mivia_domain::computation::{ComputationEvent, ComputationState};
use mivia_domain::hold::{Access, Gate, HoldReason};
use mivia_domain::ids::{
    AccountId, BatchId, ComputationId, FolderId, ModelChoice, ReportId, ReportPartId, SourceFileId,
    Timestamp,
};
use mivia_domain::label::LabelRule;
use mivia_domain::path::WinPath;
use mivia_domain::report::{PartEvent, PartState};
use mivia_domain::source_file::{SourceFileEvent, SourceFileState};

use super::TxError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewAccount {
    pub server_url: String,
    pub server_account_id: String,
    pub display_name: String,
}

pub trait AccountRepo {
    fn upsert(&mut self, account: &NewAccount) -> Result<AccountId, TxError>;
    fn activate(&mut self, id: AccountId) -> Result<(), TxError>;
}

pub trait HoldRepo {
    fn raise(
        &mut self,
        reason: HoldReason,
        since: Timestamp,
        next_probe_at: Option<Timestamp>,
    ) -> Result<(), TxError>;
    fn release(&mut self, reason: HoldReason) -> Result<(), TxError>;
    fn active(&mut self) -> Result<Vec<HoldReason>, TxError>;
    fn access(&mut self, gate: Gate) -> Result<Access, TxError>;
    fn due_for_probe(&mut self, now: Timestamp) -> Result<Vec<HoldReason>, TxError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewFolder {
    pub path: WinPath,
    pub position: u32,
    pub quiet_period_ms: i64,
    pub label_rule: LabelRule,
    pub report_settings_json: String,
}

pub trait FolderRepo {
    fn add(&mut self, folder: &NewFolder) -> Result<FolderId, TxError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Folder(FolderId),
    OneOff,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BatchModelChoice {
    pub choice: ModelChoice,
    pub names_json: String,
    pub model_version: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewBatch {
    pub origin: Origin,
    pub batch_key: String,
    pub label: String,
    pub opened_at: Timestamp,
    pub report_settings_json: String,
    pub app_version: String,
    pub model_choices: Vec<BatchModelChoice>,
}

pub trait BatchRepo {
    fn open(&mut self, batch: &NewBatch) -> Result<BatchId, TxError>;
    fn close(
        &mut self,
        id: BatchId,
        reason: ClosureReason,
        closed_at: Timestamp,
    ) -> Result<(), TxError>;
    fn open_batches(&mut self) -> Result<Vec<BatchId>, TxError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewSourceFile {
    pub batch_id: BatchId,
    pub path: WinPath,
    pub label: String,
    pub size: u64,
    pub mtime: Timestamp,
    pub next_attempt_at: Timestamp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceClaim {
    Probe,
    Upload,
}

pub trait SourceFileRepo {
    fn add(&mut self, file: &NewSourceFile) -> Result<SourceFileId, TxError>;
    fn apply(
        &mut self,
        id: SourceFileId,
        event: SourceFileEvent,
        now: Timestamp,
    ) -> Result<SourceFileState, TxError>;
    fn due(
        &mut self,
        claim: SourceClaim,
        now: Timestamp,
        exclude: &[SourceFileId],
    ) -> Result<Vec<SourceFileId>, TxError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewComputation {
    pub batch_id: BatchId,
    pub source_file_id: SourceFileId,
    pub model_choice_key: String,
    pub model_version: Option<String>,
    pub app_version: String,
    pub next_attempt_at: Timestamp,
}

pub trait ComputationRepo {
    fn add(&mut self, computation: &NewComputation) -> Result<ComputationId, TxError>;
    fn apply(
        &mut self,
        id: ComputationId,
        event: ComputationEvent,
        now: Timestamp,
    ) -> Result<ComputationState, TxError>;
    fn due_for_submit(
        &mut self,
        now: Timestamp,
        exclude: &[ComputationId],
    ) -> Result<Vec<ComputationId>, TxError>;
    fn due_for_poll(
        &mut self,
        now: Timestamp,
        exclude: &[ComputationId],
    ) -> Result<Vec<ComputationId>, TxError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewReport {
    pub batch_id: BatchId,
    pub model_choice_key: String,
    pub formats: Vec<String>,
    pub language: String,
    pub created_at: Timestamp,
    pub app_version: String,
    pub members: Vec<(ComputationId, String)>,
}

pub trait ReportRepo {
    fn create(&mut self, report: &NewReport) -> Result<ReportId, TxError>;
    fn parts(&mut self, id: ReportId) -> Result<Vec<ReportPartId>, TxError>;
    fn apply_part(
        &mut self,
        id: ReportPartId,
        event: PartEvent,
        now: Timestamp,
    ) -> Result<PartState, TxError>;
    fn set_stale(&mut self, id: ReportId, stale: bool) -> Result<(), TxError>;
    fn due_for_request(&mut self, exclude: &[ReportPartId]) -> Result<Vec<ReportPartId>, TxError>;
    fn due_for_poll(
        &mut self,
        now: Timestamp,
        exclude: &[ReportPartId],
    ) -> Result<Vec<ReportPartId>, TxError>;
}

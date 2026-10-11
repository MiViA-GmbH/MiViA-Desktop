use mivia_domain::computation::{ComputationEvent, JobOutcome};
use mivia_domain::ids::{BatchId, ComputationId, Fingerprint, SourceFileId, Timestamp};
use mivia_domain::path::WinPath;
use mivia_domain::source_file::SourceFileEvent;

use super::TxError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Fairness {
    pub batch: BatchId,
    pub one_off: bool,
    pub opened_at: Timestamp,
    pub arrival_seq: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UploadRow {
    pub id: SourceFileId,
    pub fairness: Fairness,
    pub path: WinPath,
    pub root: WinPath,
    pub size: u64,
    pub fingerprint: Fingerprint,
    pub attempts: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmitRow {
    pub id: ComputationId,
    pub fairness: Fairness,
    pub choice_key: String,
    pub image_id: String,
    pub attempts: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PollRow {
    pub id: ComputationId,
    pub job_id: String,
    pub submitted_at: Timestamp,
    pub resubmitted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewAttempt {
    pub key: String,
    pub choice_key: String,
    pub started_at: Timestamp,
    pub server_offset_ms: Option<i64>,
    pub members: Vec<(ComputationId, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttemptRow {
    pub key: String,
    pub choice_key: String,
    pub started_at: Timestamp,
    pub server_offset_ms: Option<i64>,
    pub members: Vec<(ComputationId, String)>,
    pub attempts: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogueRow {
    pub model_id: String,
    pub customization_id: Option<String>,
    pub names_json: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobRecord {
    pub job_id: String,
    pub outcome: JobOutcome,
    pub result_id: Option<String>,
    pub submitted_at: Timestamp,
    pub next_poll_at: Timestamp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clamp {
    pub retry_until: Timestamp,
    pub poll_until: Timestamp,
    pub probe_until: Timestamp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Retry {
    pub at: Timestamp,
    pub attempts: u32,
}

pub trait PipelineRepo {
    fn upload_due(
        &mut self,
        now: Timestamp,
        exclude: &[SourceFileId],
        skip_batches: &[BatchId],
    ) -> Result<Vec<UploadRow>, TxError>;
    fn upload_canary(&mut self, now: Timestamp) -> Result<Vec<UploadRow>, TxError>;
    fn uploaded(&mut self, id: SourceFileId, image_id: &str, now: Timestamp)
    -> Result<(), TxError>;
    fn upload_retry(
        &mut self,
        id: SourceFileId,
        retry: Retry,
        now: Timestamp,
    ) -> Result<(), TxError>;
    fn file_failed(
        &mut self,
        id: SourceFileId,
        event: SourceFileEvent,
        now: Timestamp,
    ) -> Result<(), TxError>;
    fn image_gone(&mut self, image_id: &str, now: Timestamp) -> Result<(), TxError>;
    fn submit_due(
        &mut self,
        now: Timestamp,
        exclude: &[ComputationId],
        skip_batches: &[BatchId],
    ) -> Result<Vec<SubmitRow>, TxError>;
    fn submit_canary(&mut self, now: Timestamp) -> Result<Vec<SubmitRow>, TxError>;
    fn next_poll(&mut self) -> Result<Option<Timestamp>, TxError>;
    fn begin_attempt(&mut self, attempt: &NewAttempt, now: Timestamp) -> Result<(), TxError>;
    fn record_job(
        &mut self,
        id: ComputationId,
        job: &JobRecord,
        now: Timestamp,
    ) -> Result<(), TxError>;
    fn computation_event(
        &mut self,
        id: ComputationId,
        event: ComputationEvent,
        now: Timestamp,
    ) -> Result<(), TxError>;
    fn computation_retry(&mut self, id: ComputationId, retry: Retry) -> Result<(), TxError>;
    fn recovery_due(
        &mut self,
        now: Timestamp,
        exclude: &[String],
    ) -> Result<Vec<AttemptRow>, TxError>;
    fn job_known(&mut self, job_id: &str) -> Result<bool, TxError>;
    fn preview_source(&mut self, id: SourceFileId) -> Result<Option<WinPath>, TxError>;
    fn preview_job(&mut self, id: ComputationId) -> Result<Option<String>, TxError>;
    fn preview_model(&mut self, id: ComputationId) -> Result<Option<String>, TxError>;
    fn poll_due(
        &mut self,
        now: Timestamp,
        exclude: &[ComputationId],
        limit: u32,
    ) -> Result<Vec<PollRow>, TxError>;
    fn set_next_poll(&mut self, id: ComputationId, at: Timestamp) -> Result<(), TxError>;
    fn computing_count(&mut self) -> Result<u64, TxError>;
    fn recover_after_restart(&mut self, now: Timestamp) -> Result<(), TxError>;
    fn clamp_schedule(&mut self, clamp: Clamp) -> Result<(), TxError>;
    fn replace_catalogue(&mut self, rows: &[CatalogueRow], now: Timestamp) -> Result<(), TxError>;
    fn queued_choices(&mut self) -> Result<Vec<(ComputationId, String)>, TxError>;
}

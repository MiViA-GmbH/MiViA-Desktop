use mivia_domain::ids::{BatchId, ComputationId, Fingerprint, ReportId, ReportPartId, Timestamp};
use mivia_domain::naming::Language;
use mivia_domain::output::{Format, ProbeResult, ReportFolder};
use mivia_domain::path::WinPath;
use mivia_domain::report::{OmittedReason, PartEvent, PartState, SaveError};

use super::TxError;
use crate::fs::Staged;

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaitingReport {
    pub id: ReportId,
    pub settings_json: String,
    pub pending: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportSpec {
    pub formats: Vec<Format>,
    pub language: Language,
    pub decimal_comma: bool,
    pub include_images: bool,
    pub timezone: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestRow {
    pub id: ReportPartId,
    pub format: Format,
    pub part: (u32, u32),
    pub language: Language,
    pub include_images: bool,
    pub decimal_comma: bool,
    pub timezone: String,
    pub members: Vec<(String, String)>,
    pub omitted: Vec<(String, OmittedReason)>,
    pub attempts: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PollRow {
    pub id: ReportPartId,
    pub server_id: String,
    pub format: Format,
    pub requested_at: Timestamp,
    pub rerequests: u32,
    pub split: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveRow {
    pub id: ReportPartId,
    pub format: Format,
    pub part: (u32, u32),
    pub language: Language,
    pub label: String,
    pub closed_at: Timestamp,
    pub analysis_json: String,
    pub variant_json: Option<String>,
    pub root: Option<WinPath>,
    pub report_folder: Option<ReportFolder>,
    pub probe: Option<ProbeResult>,
    pub first_member: Option<WinPath>,
    pub staging: WinPath,
    pub size: u64,
    pub sha256: Fingerprint,
    pub intended: Option<WinPath>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rerequest {
    Expired,
    Cancelled,
    TryAgain,
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
    fn waiting(&mut self) -> Result<Vec<WaitingReport>, TxError>;
    fn build(&mut self, id: ReportId, spec: &ReportSpec, now: Timestamp) -> Result<u32, TxError>;
    fn in_flight(&mut self) -> Result<bool, TxError>;
    fn next_request(&mut self, now: Timestamp) -> Result<Option<RequestRow>, TxError>;
    fn begin_request(&mut self, id: ReportPartId, now: Timestamp) -> Result<(), TxError>;
    fn requested(
        &mut self,
        id: ReportPartId,
        server_id: &str,
        next_poll: Timestamp,
    ) -> Result<(), TxError>;
    fn request_retry(
        &mut self,
        id: ReportPartId,
        at: Timestamp,
        attempts: u32,
    ) -> Result<(), TxError>;
    fn poll_due(&mut self, now: Timestamp) -> Result<Vec<PollRow>, TxError>;
    fn next_poll(&mut self) -> Result<Option<Timestamp>, TxError>;
    fn set_poll(&mut self, id: ReportPartId, at: Timestamp) -> Result<(), TxError>;
    fn rerequest(
        &mut self,
        id: ReportPartId,
        why: Rerequest,
        now: Timestamp,
    ) -> Result<PartState, TxError>;
    fn split(&mut self, id: ReportPartId, now: Timestamp) -> Result<PartState, TxError>;
    fn members_lost(&mut self, id: ReportPartId, job_ids: &[String]) -> Result<u32, TxError>;
    fn downloaded(
        &mut self,
        id: ReportPartId,
        staging: &WinPath,
        staged: &Staged,
    ) -> Result<(), TxError>;
    fn save_due(&mut self, now: Timestamp) -> Result<Vec<SaveRow>, TxError>;
    fn save_row(&mut self, id: ReportPartId) -> Result<SaveRow, TxError>;
    fn next_save(&mut self) -> Result<Option<Timestamp>, TxError>;
    fn intend(&mut self, id: ReportPartId, path: &WinPath) -> Result<(), TxError>;
    fn saved(&mut self, id: ReportPartId, path: &WinPath, now: Timestamp) -> Result<(), TxError>;
    fn save_failed(
        &mut self,
        id: ReportPartId,
        error: SaveError,
        now: Timestamp,
    ) -> Result<PartState, TxError>;
    fn recover_parts(&mut self, now: Timestamp) -> Result<(), TxError>;
}

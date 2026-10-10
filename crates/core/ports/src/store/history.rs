use mivia_domain::computation::ComputationState;
use mivia_domain::ids::{BatchId, ComputationId, Fingerprint, ReportId, ReportPartId, Timestamp};
use mivia_domain::naming::Language;
use mivia_domain::path::WinPath;
use mivia_domain::report::OmittedReason;

use super::TxError;
use super::reports::ReportSpec;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    User,
    Folder,
    SendTo,
    Automatic,
}

impl Trigger {
    pub const fn code(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Folder => "folder",
            Self::SendTo => "send_to",
            Self::Automatic => "automatic",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewEvent {
    pub at: Timestamp,
    pub batch: Option<BatchId>,
    pub report: Option<ReportId>,
    pub action: &'static str,
    pub trigger: Trigger,
    pub before: Option<String>,
    pub after: Option<String>,
}

impl NewEvent {
    pub const fn new(action: &'static str, trigger: Trigger, at: Timestamp) -> Self {
        Self {
            at,
            batch: None,
            report: None,
            action,
            trigger,
            before: None,
            after: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Actor {
    pub windows_user: String,
    pub tz: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberCandidate {
    pub computation: ComputationId,
    pub batch: BatchId,
    pub choice_key: String,
    pub label: String,
    pub arrival: i64,
    pub state: ComputationState,
    pub file_rejected: bool,
    pub done_at: Option<Timestamp>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomMember {
    pub computation: ComputationId,
    pub label: String,
    pub omitted: Option<OmittedReason>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewCustomReport {
    pub name: String,
    pub choice_key: String,
    pub spec: ReportSpec,
    pub folder: WinPath,
    pub created_at: Timestamp,
    pub app_version: String,
    pub members: Vec<CustomMember>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MovablePart {
    pub id: ReportPartId,
    pub path: WinPath,
    pub size: u64,
    pub sha256: Fingerprint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveSet {
    pub batch: Option<BatchId>,
    pub language: Language,
    pub replacement_saved: bool,
    pub moved: bool,
    pub parts: Vec<MovablePart>,
    pub unsaved: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Moved {
    ToReplaced(Timestamp),
    Restored(Timestamp),
}

pub trait HistoryRepo {
    fn log(&mut self, event: &NewEvent) -> Result<(), TxError>;
    fn rename(&mut self, id: BatchId, label: &str, now: Timestamp) -> Result<(), TxError>;
    fn candidates(&mut self, batches: &[BatchId]) -> Result<Vec<MemberCandidate>, TxError>;
    fn create_custom(&mut self, report: &NewCustomReport) -> Result<ReportId, TxError>;
    fn update_stale(&mut self, id: ReportId, now: Timestamp) -> Result<ReportId, TxError>;
    fn keep_as_is(&mut self, id: ReportId, now: Timestamp) -> Result<Option<BatchId>, TxError>;
    fn move_set(&mut self, id: ReportId) -> Result<MoveSet, TxError>;
    fn moved(&mut self, part: ReportPartId, path: &WinPath, how: Moved) -> Result<(), TxError>;
}

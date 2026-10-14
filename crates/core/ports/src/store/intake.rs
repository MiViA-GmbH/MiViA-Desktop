use mivia_domain::ids::{BatchId, FolderId, SourceFileId, Timestamp};
use mivia_domain::intake::BatchKey;
use mivia_domain::label::LabelRule;
use mivia_domain::output::{ProbeResult, ReportFolder};
use mivia_domain::path::WinPath;
use mivia_domain::readiness::Attempts;
use mivia_domain::source_file::SourceFileState;

use super::TxError;
use crate::fs::{ImageRead, WalkEntry};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderRow {
    pub id: FolderId,
    pub path: WinPath,
    pub active: bool,
    pub quiet_ms: i64,
    pub label_rule: LabelRule,
    pub missing_since: Option<Timestamp>,
    pub waiting_since: Option<Timestamp>,
    pub report_folder: ReportFolder,
    pub probe: Option<ProbeResult>,
    pub marker_conflict: Option<MarkerPeer>,
    pub marker_dismissed: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkerPeer {
    pub marker_id: String,
    pub machine: String,
    pub user: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FolderChange {
    Active(bool),
    Probe(ProbeResult),
    Missing(Option<Timestamp>),
    Waiting(Option<Timestamp>),
    Marker(Option<MarkerPeer>),
    DismissMarker,
    Relocated(WinPath),
    Removed(Timestamp),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    pub id: SourceFileId,
    pub batch_id: BatchId,
    pub path: WinPath,
    pub state: SourceFileState,
    pub size: u64,
    pub mtime: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detection {
    pub folder: FolderId,
    pub key: BatchKey,
    pub series_label: String,
    pub label: String,
    pub path: WinPath,
    pub next_attempt_at: Timestamp,
    pub activity: Timestamp,
    pub app_version: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Detected {
    pub id: SourceFileId,
    pub batch: BatchId,
    pub files: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeRow {
    pub id: SourceFileId,
    pub root: WinPath,
    pub path: WinPath,
    pub attempts: Attempts,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownPath {
    pub path: WinPath,
    pub size: u64,
    pub mtime: Timestamp,
    pub baseline: bool,
    pub given_up: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sighting {
    Unchanged { copy_of: Option<SourceFileId> },
    Fresh(ContentLookup),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentLookup {
    pub earlier_in_batch: Option<SourceFileId>,
    pub in_ledger: bool,
    pub same_path: bool,
    pub other_paths: Vec<KnownPath>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Classified {
    Copy(SourceFileId),
    KnownFile,
    KnownContent,
    New,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenBatchRow {
    pub id: BatchId,
    pub folder: FolderId,
    pub key: String,
    pub opened_at: Timestamp,
    pub last_member_at: Timestamp,
    pub files: u32,
    pub detected: u32,
    pub quiet_ms: i64,
}

pub trait IntakeRepo {
    fn live_members(&mut self, folder: FolderId) -> Result<Vec<Member>, TxError>;
    fn detect(&mut self, detection: &Detection) -> Result<Detected, TxError>;
    fn touch(&mut self, id: SourceFileId, next_attempt_at: Timestamp) -> Result<(), TxError>;
    fn rebase(&mut self, id: SourceFileId, path: &WinPath, label: &str) -> Result<(), TxError>;
    fn remove(&mut self, id: SourceFileId, now: Timestamp) -> Result<(), TxError>;
    fn content_changed(&mut self, id: SourceFileId, now: Timestamp) -> Result<(), TxError>;
    fn claim_probes(
        &mut self,
        now: Timestamp,
        exclude: &[SourceFileId],
        limit: u32,
    ) -> Result<Vec<ProbeRow>, TxError>;
    fn next_probe_at(&mut self) -> Result<Option<Timestamp>, TxError>;
    fn retry(&mut self, id: SourceFileId, at: Timestamp, attempts: Attempts)
    -> Result<(), TxError>;
    fn ready(
        &mut self,
        id: SourceFileId,
        read: &ImageRead,
        now: Timestamp,
    ) -> Result<Sighting, TxError>;
    fn classify(
        &mut self,
        id: SourceFileId,
        outcome: Classified,
        now: Timestamp,
    ) -> Result<(), TxError>;
    fn known_paths(&mut self, folder: FolderId) -> Result<Vec<KnownPath>, TxError>;
    fn record_baseline(
        &mut self,
        folder: FolderId,
        files: &[WalkEntry],
        now: Timestamp,
    ) -> Result<(), TxError>;
    fn take_baseline(
        &mut self,
        folder: FolderId,
        since: Timestamp,
    ) -> Result<Vec<WinPath>, TxError>;
    fn open_batches(&mut self) -> Result<Vec<OpenBatchRow>, TxError>;
    fn member_paths(&mut self, batch: BatchId) -> Result<Vec<WinPath>, TxError>;
    fn relabel(&mut self, batch: BatchId, label: &str) -> Result<(), TxError>;
    fn rekey(&mut self, batch: BatchId, key: &BatchKey, label: &str) -> Result<(), TxError>;
    fn move_detected(&mut self, from: BatchId, now: Timestamp) -> Result<(), TxError>;
}

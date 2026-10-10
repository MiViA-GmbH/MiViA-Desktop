use serde::{Deserialize, Serialize};

use super::screens::{AnalysisView, Language};
use super::views::BatchSummary;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum HistorySort {
    #[default]
    Finished,
    Name,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum HistoryStatus {
    Problems,
    OutOfDate,
    Active,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct HistoryFilter {
    pub q: Option<String>,
    pub folder: Option<String>,
    pub manual: bool,
    pub analysis: Option<String>,
    pub status: Option<HistoryStatus>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub sort: HistorySort,
    pub ascending: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct HistoryCursor {
    pub key: String,
    pub id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum MatchKind {
    Image,
    Report,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct HistoryMatch {
    pub kind: MatchKind,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct HistoryRow {
    pub summary: BatchSummary,
    pub analyses: Vec<AnalysisView>,
    pub matched: Option<HistoryMatch>,
    pub saved_part: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub revision: String,
    pub head: Vec<HistoryRow>,
    pub rows: Vec<HistoryRow>,
    pub next: Option<HistoryCursor>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CustomReportRow {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub images: u32,
    pub analysis: AnalysisView,
    pub state: String,
    pub stale: bool,
    pub problem: bool,
    pub saved_part: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CustomReportsPage {
    pub revision: String,
    pub rows: Vec<CustomReportRow>,
    pub next: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct HistoryEventView {
    pub id: String,
    pub at: String,
    pub tz: String,
    pub windows_user: String,
    pub action: String,
    pub trigger: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct HistoryEventsPage {
    pub revision: String,
    pub events: Vec<HistoryEventView>,
    pub next: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ReportOrder {
    Name,
    Date,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct OmittedView {
    pub label: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CustomAnalysisView {
    pub analysis: AnalysisView,
    pub included: u32,
    pub omitted: Vec<OmittedView>,
    pub parts: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CustomReportDraft {
    pub images: u32,
    pub too_many: bool,
    pub analyses: Vec<CustomAnalysisView>,
    pub language: Language,
    pub folder: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CustomReportInput {
    pub series: Vec<String>,
    pub name: String,
    pub formats: Vec<String>,
    pub language: Language,
    pub decimal_comma: bool,
    pub include_images: bool,
    pub order: ReportOrder,
    pub folder_pick: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct BackupView {
    pub id: String,
    pub date: String,
    pub in_folder: bool,
    pub series: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct BackupsView {
    pub folder: Option<String>,
    pub last_copy: Option<String>,
    pub backups: Vec<BackupView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ExportView {
    pub folder: String,
    pub files: Vec<String>,
}

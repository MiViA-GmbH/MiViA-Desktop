use serde::{Deserialize, Serialize};

use super::screens::{AnalysisView, NeedsAttention};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum OriginView {
    Folder { folder_id: String },
    OneOff,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct BatchCounts {
    pub files: u32,
    pub done: u32,
    pub failed: u32,
    pub problems: u32,
    pub active: u32,
    pub reports_saved: u32,
    pub reports_problem: u32,
    pub reports_total: u32,
    pub reports_stale: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct BatchSummary {
    pub id: String,
    pub label: String,
    pub origin: OriginView,
    pub open: bool,
    pub opened_at: String,
    pub closed_at: Option<String>,
    pub counts: BatchCounts,
    pub needs_attention: bool,
    pub closure_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct HistoryCursor {
    pub sort_at: String,
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub revision: String,
    pub head: Vec<BatchSummary>,
    pub rows: Vec<BatchSummary>,
    pub next: Option<HistoryCursor>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct NeedsAttentionView {
    pub revision: String,
    pub items: Vec<NeedsAttention>,
    pub total: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct BatchView {
    pub revision: String,
    pub summary: BatchSummary,
    pub analyses: Vec<AnalysisView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ComputationItem {
    pub id: String,
    pub model_choice_key: String,
    pub state: String,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct BatchItem {
    pub id: String,
    pub label: String,
    pub state: String,
    pub reason: Option<String>,
    pub computations: Vec<ComputationItem>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct BatchItemsPage {
    pub revision: String,
    pub items: Vec<BatchItem>,
    pub next: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FolderView {
    pub id: String,
    pub name: String,
    pub path: String,
    pub position: u32,
    pub paused: bool,
    pub missing: bool,
    pub quiet_seconds: u32,
    pub report_folder: Option<String>,
    pub open_series: Option<BatchSummary>,
    pub last_series: Option<BatchSummary>,
    pub latest_report: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FoldersView {
    pub revision: String,
    pub folders: Vec<FolderView>,
}

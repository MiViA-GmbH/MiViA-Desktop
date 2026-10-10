use serde::{Deserialize, Serialize};

use super::setup::{ArrivalPreset, ModelChoiceView, ReportMode};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum FolderBlock {
    NotFound,
    TooBroad,
    Nested { name: String },
    InsideReports,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum FolderNote {
    General,
    OneDrive,
    Network,
    MappedDrive { letter: String, unc: String },
    WatchedElsewhere { machine: String, user: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FolderPickView {
    pub display: String,
    pub name: String,
    pub block: Option<FolderBlock>,
    pub notes: Vec<FolderNote>,
    pub existing: u32,
    pub existing_more: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ReportFolderView {
    pub mode: ReportMode,
    pub display: String,
    pub subfolder_name: String,
    pub denied: Option<String>,
    pub long_path: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CustomizationView {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CatalogueModelView {
    pub model_id: String,
    pub name: String,
    pub customizations: Vec<CustomizationView>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CatalogueView {
    pub loaded: bool,
    pub models: Vec<CatalogueModelView>,
    pub preselected: Vec<ModelChoiceView>,
    pub legacy_gone: Vec<String>,
    pub legacy_custom_gone: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum PickPurpose {
    Input,
    Reports,
    Legacy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FolderPick {
    pub pick_id: String,
    pub folder: FolderPickView,
    pub report: Option<ReportFolderView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct NewFolderInput {
    pub folder_pick: String,
    pub preset: ArrivalPreset,
    pub report_mode: ReportMode,
    pub report_pick: Option<String>,
    pub models: Vec<ModelChoiceView>,
    pub include_existing: bool,
}

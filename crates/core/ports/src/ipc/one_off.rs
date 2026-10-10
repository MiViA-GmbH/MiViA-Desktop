use serde::{Deserialize, Serialize};

use super::screens::Language;
use super::setup::ModelChoiceView;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum NotIncludedReason {
    Missing,
    NotImage,
    ZipTemp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct NotIncludedView {
    pub name: String,
    pub reason: NotIncludedReason,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ImageGroupView {
    pub folder: String,
    pub names: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct OneOffView {
    pub count: u32,
    pub groups: Vec<ImageGroupView>,
    pub not_included: Vec<NotIncludedView>,
    pub subfolders: u32,
    pub too_many: bool,
    pub label: String,
    pub last_choices: Vec<ModelChoiceView>,
    pub last_chosen_at: Option<String>,
    pub report_folder: String,
    pub report_fallback: bool,
    pub language: Language,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct OneOffInput {
    pub label: String,
    pub models: Vec<ModelChoiceView>,
    pub pdf: bool,
    pub csv: bool,
    pub language: Language,
    pub report_pick: Option<String>,
}

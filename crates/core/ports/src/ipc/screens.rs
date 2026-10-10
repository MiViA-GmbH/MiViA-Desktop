use std::collections::BTreeMap;

use mivia_domain::hold::{HoldReason, KeyProblem};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum HoldKind {
    Offline,
    ServiceUnavailable,
    NetworkBlocked,
    AccessKey,
    AccountBlocked,
    AdminScope,
    License,
    ClientTooOld,
}

impl From<HoldReason> for HoldKind {
    fn from(reason: HoldReason) -> Self {
        match reason {
            HoldReason::Offline => Self::Offline,
            HoldReason::ServiceUnavailable => Self::ServiceUnavailable,
            HoldReason::NetworkBlocked(_) => Self::NetworkBlocked,
            HoldReason::AccessKeyInvalid(KeyProblem::Invalid) => Self::AccessKey,
            HoldReason::AccessKeyInvalid(KeyProblem::AccountBlocked) => Self::AccountBlocked,
            HoldReason::AccessKeyInvalid(KeyProblem::AdminScope) => Self::AdminScope,
            HoldReason::LicenseInvalid(_) => Self::License,
            HoldReason::ClientTooOld => Self::ClientTooOld,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct HoldView {
    pub kind: HoldKind,
    pub since: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct HoldsView {
    pub revision: String,
    pub holds: Vec<HoldView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct AttentionSeries {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NeedsAttention {
    FolderNotFound {
        folder_id: String,
        folder_name: String,
        path: String,
        unreachable_since: Option<String>,
    },
    WatchedElsewhere {
        folder_id: String,
        folder_name: String,
        machine: String,
        user: String,
    },
    ReportNotSaved {
        series: AttentionSeries,
        part_id: String,
    },
    ReportFailed {
        series: AttentionSeries,
        part_id: String,
    },
    ReportStale {
        series: AttentionSeries,
    },
    SeriesProblems {
        series: AttentionSeries,
        problems: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct AnalysisView {
    pub key: String,
    pub model_id: String,
    pub names: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ReportPartView {
    pub id: String,
    pub part_no: u32,
    pub part_count: u32,
    pub format: String,
    pub member_from: u32,
    pub member_to: u32,
    pub state: String,
    pub reason: Option<String>,
    pub file_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ReportView {
    pub id: String,
    pub analysis_key: String,
    pub state: String,
    pub stale: bool,
    pub replaced_by: Option<String>,
    pub moved: bool,
    pub parts: Vec<ReportPartView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SeriesReportsView {
    pub revision: String,
    pub reports: Vec<ReportView>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum Language {
    En,
    De,
}

impl From<mivia_domain::naming::Language> for Language {
    fn from(language: mivia_domain::naming::Language) -> Self {
        match language {
            mivia_domain::naming::Language::En => Self::En,
            mivia_domain::naming::Language::De => Self::De,
        }
    }
}

impl From<Language> for mivia_domain::naming::Language {
    fn from(language: Language) -> Self {
        match language {
            Language::En => Self::En,
            Language::De => Self::De,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum LanguageSetting {
    #[default]
    Auto,
    En,
    De,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct LocaleView {
    pub language: Language,
    pub setting: LanguageSetting,
    pub format_locale: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct AccountView {
    pub display_name: String,
    pub server_url: String,
    pub server_account_id: String,
    pub email: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum OpenTarget {
    Report { part_id: String },
    RevealReport { part_id: String },
    Link { link: super::support::KnownLink },
}

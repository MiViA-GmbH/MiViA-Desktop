use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ReportTrigger {
    Manual,
    Error,
    Crash,
}

impl ReportTrigger {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Error => "error",
            Self::Crash => "crash",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ReportRange {
    Session,
    ThreeDays,
    FourteenDays,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ProblemReportInput {
    pub description: String,
    pub range: ReportRange,
    pub include_names: bool,
    pub trigger: ReportTrigger,
    pub signature: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct BundleEntryView {
    pub name: String,
    pub bytes: u32,
    pub lines: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ProblemReportPreview {
    pub report_id: String,
    pub reference: String,
    pub entries: Vec<BundleEntryView>,
    pub total_bytes: u32,
    pub truncated: bool,
    pub from: String,
    pub to: String,
    pub session_ids: Vec<String>,
    pub install_id: String,
    pub first_errors: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SendOutcome {
    Sent { reference: String },
    Queued { reference: String },
    SaveAndEmail { reference: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum PendingState {
    Waiting,
    SaveAndEmail,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PendingReportView {
    pub report_id: String,
    pub reference: String,
    pub created_at: String,
    pub state: PendingState,
    pub attempts: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum IssueReporting {
    On,
    MailtoOnly,
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum PolicySource {
    Machine,
    User,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PolicyRow {
    pub name: String,
    pub value: Option<String>,
    pub source: Option<PolicySource>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SupportView {
    pub reporting: IssueReporting,
    pub can_send: bool,
    pub contact: String,
    pub policies: Vec<PolicyRow>,
    pub version: String,
    pub session_id: String,
    pub install_id: String,
    pub pending: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum KnownLink {
    Support,
    Releases,
    License,
    AccessKeys,
    InfoMail,
    Notices,
}

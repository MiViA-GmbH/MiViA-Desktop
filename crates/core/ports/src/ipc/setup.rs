use serde::{Deserialize, Serialize};

use super::folders::{FolderPickView, ReportFolderView};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum SetupStep {
    #[default]
    Welcome,
    Legacy,
    Key,
    Folder,
    Series,
    Models,
    Finish,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ArrivalPreset {
    Microscope,
    WholeFolder,
    #[default]
    ByHand,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ReportMode {
    #[default]
    Subfolder,
    Beside,
    Custom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum LegacyTake {
    All,
    KeyOnly,
    Nothing,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ModelChoiceView {
    pub model_id: String,
    pub customization_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ConnectionNote {
    LicenseInactive,
    StoreFailed,
    Imported,
    Browser,
    Code,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ConnectionView {
    pub name: String,
    pub email: String,
    pub notes: Vec<ConnectionNote>,
    pub remaining_uploads: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ConnectOutcome {
    Connected {
        connection: ConnectionView,
        other_account: bool,
        clipboard_cleared: bool,
    },
    Malformed,
    Rejected,
    AccountBlocked,
    AdminScope,
    MissingScopes,
    Offline {
        details: String,
    },
    SwitchAccount {
        current_name: String,
        new_name: String,
    },
    Cancelled,
    SignInUnavailable,
    SignInTimedOut,
    SignInDenied,
    SignInExpired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum LegacyFlag {
    HasKey,
    Running,
    Network,
    Unreadable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct LegacyView {
    pub folder: String,
    pub input_folder: Option<String>,
    pub model_count: u32,
    pub flags: Vec<LegacyFlag>,
    pub take: Option<LegacyTake>,
}

#[derive(Clone, Copy, Hash, PartialOrd, Ord, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum SetupOption {
    SkipFolder,
    IncludeExisting,
    Autostart,
    Explorer,
    Telemetry,
    LegacyStop,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SetupLocks {
    pub language: bool,
    pub legacy_search: bool,
    pub autostart: Option<bool>,
    pub explorer: Option<bool>,
    pub telemetry: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SetupView {
    pub completed: bool,
    pub step: SetupStep,
    pub resumed: bool,
    pub host: String,
    pub support_contact: String,
    pub detected_language: String,
    pub locks: SetupLocks,
    pub legacy: Option<LegacyView>,
    pub legacy_folder_missing: Option<String>,
    pub connection: Option<ConnectionView>,
    pub folder: Option<FolderPickView>,
    pub preset: ArrivalPreset,
    pub report: Option<ReportFolderView>,
    pub models: Vec<ModelChoiceView>,
    pub options: Vec<SetupOption>,
    pub images_sent: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SetupInput {
    pub step: SetupStep,
    pub preset: ArrivalPreset,
    pub report_mode: ReportMode,
    pub models: Vec<ModelChoiceView>,
    pub options: Vec<SetupOption>,
    pub legacy_take: Option<LegacyTake>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum WaitingReason {
    NoModels,
    License,
    Offline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum LegacyOutcome {
    Stopped,
    Close,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum DoneProblem {
    Autostart,
    Explorer,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DoneView {
    pub name: String,
    pub email: String,
    pub folder: Option<String>,
    pub folder_name: Option<String>,
    pub models: Vec<String>,
    pub report_folder: Option<String>,
    pub existing: u32,
    pub waiting: Vec<WaitingReason>,
    pub problems: Vec<DoneProblem>,
    pub explorer: bool,
    pub legacy: Option<LegacyOutcome>,
    pub legacy_startup: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum StartupState {
    On,
    Off,
    TurnedOffInTaskManager,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct IntegrationView {
    pub available: bool,
    pub autostart: StartupState,
    pub explorer: bool,
    pub locks: SetupLocks,
}

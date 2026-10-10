use std::collections::BTreeSet;

use mivia_domain::ids::{FolderId, MINUTE_MS, ModelChoice};
use mivia_domain::path::WinPath;
use mivia_ports::ipc::folders::{FolderBlock, FolderNote};
use mivia_ports::ipc::setup::{
    ArrivalPreset, ConnectionNote, ConnectionView, LegacyTake, ModelChoiceView, ReportMode,
    SetupInput, SetupOption, SetupStep,
};
use mivia_ports::store::{AccountScope, NewAccount, SettingsScope, TxError, WriteTx};
use serde::{Deserialize, Serialize};

use crate::intake::{PreparedFolder, insert_folder};
use crate::settings::{GlobalSettings, Integration, write_global};

pub const DRAFT_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftConnection {
    pub origin: String,
    pub account_id: String,
    pub name: String,
    pub email: String,
    pub notes: Vec<ConnectionNote>,
    pub remaining_uploads: Option<u32>,
}

impl DraftConnection {
    pub fn view(&self) -> ConnectionView {
        ConnectionView {
            name: self.name.clone(),
            email: self.email.clone(),
            notes: self.notes.clone(),
            remaining_uploads: self.remaining_uploads,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SetupDraft {
    pub step: SetupStep,
    pub connection: Option<DraftConnection>,
    pub legacy_folder: Option<String>,
    pub legacy_take: Option<LegacyTake>,
    pub folder: Option<String>,
    pub preset: ArrivalPreset,
    pub report_mode: ReportMode,
    pub report_custom: Option<String>,
    pub models: Vec<String>,
    pub options: BTreeSet<SetupOption>,
}

impl Default for SetupDraft {
    fn default() -> Self {
        Self {
            step: SetupStep::Welcome,
            connection: None,
            legacy_folder: None,
            legacy_take: None,
            folder: None,
            preset: ArrivalPreset::ByHand,
            report_mode: ReportMode::Subfolder,
            report_custom: None,
            models: Vec::new(),
            options: [
                SetupOption::Autostart,
                SetupOption::Explorer,
                SetupOption::Telemetry,
                SetupOption::LegacyStop,
            ]
            .into(),
        }
    }
}

impl SetupDraft {
    pub fn parse(json: Option<&str>) -> Option<Self> {
        json.and_then(|json| serde_json::from_str(json).ok())
    }

    pub fn has(&self, option: SetupOption) -> bool {
        self.options.contains(&option)
    }

    pub fn folder_path(&self) -> Option<WinPath> {
        self.folder
            .as_deref()
            .and_then(|text| WinPath::parse(text).ok())
    }

    pub fn choices(&self) -> Vec<ModelChoice> {
        self.models
            .iter()
            .filter_map(|key| ModelChoice::from_key(key))
            .collect()
    }

    pub fn model_views(&self) -> Vec<ModelChoiceView> {
        self.choices()
            .into_iter()
            .map(|choice| ModelChoiceView {
                model_id: choice.model_id,
                customization_id: choice.customization_id,
            })
            .collect()
    }

    pub fn apply(&mut self, input: SetupInput) {
        self.step = input.step;
        self.preset = input.preset;
        self.report_mode = input.report_mode;
        self.models = keys(&input.models);
        self.options = input.options.into_iter().collect();
        self.legacy_take = input.legacy_take;
    }

    pub fn connect(&mut self, connection: DraftConnection) -> bool {
        let other = self
            .connection
            .as_ref()
            .is_some_and(|old| old.account_id != connection.account_id);
        if other {
            self.models.clear();
        }
        self.connection = Some(connection);
        other
    }
}

pub fn keys(models: &[ModelChoiceView]) -> Vec<String> {
    models
        .iter()
        .map(|view| {
            ModelChoice {
                model_id: view.model_id.clone(),
                customization_id: view.customization_id.clone(),
            }
            .key()
        })
        .collect()
}

pub const fn quiet_ms(preset: ArrivalPreset) -> i64 {
    match preset {
        ArrivalPreset::Microscope => 15 * MINUTE_MS,
        ArrivalPreset::WholeFolder => MINUTE_MS,
        ArrivalPreset::ByHand => 5 * MINUTE_MS,
    }
}

pub fn read_draft(tx: &mut dyn WriteTx) -> Result<SetupDraft, TxError> {
    Ok(SetupDraft::parse(tx.settings(SettingsScope::SetupDraft)?.as_deref()).unwrap_or_default())
}

pub fn write_draft(tx: &mut dyn WriteTx, draft: &SetupDraft) -> Result<(), TxError> {
    let json = serde_json::to_string(draft).map_err(|error| TxError::Sql(error.to_string()))?;
    tx.set_settings(SettingsScope::SetupDraft, DRAFT_VERSION, &json)
}

pub struct Completion {
    pub account: NewAccount,
    pub folder: Option<PreparedFolder>,
    pub telemetry: bool,
    pub integration: Integration,
}

pub fn complete(
    tx: &mut dyn WriteTx,
    completion: &Completion,
) -> Result<(AccountScope, Option<FolderId>), TxError> {
    let id = tx.accounts().upsert(&completion.account)?;
    tx.accounts().activate(id)?;
    let scope = AccountScope(id);
    let folder = match &completion.folder {
        Some(prepared) => Some(insert_folder(tx, scope, prepared)?),
        None => None,
    };
    let mut global = GlobalSettings::parse(tx.settings(SettingsScope::Global)?.as_deref());
    global.setup_completed = true;
    global.telemetry = completion.telemetry;
    global.integration = completion.integration;
    write_global(tx, &global)?;
    tx.clear_settings(SettingsScope::SetupDraft)?;
    Ok((scope, folder))
}

pub struct KnownRoots {
    pub windows: Vec<WinPath>,
    pub profile: Option<WinPath>,
    pub general: Vec<WinPath>,
    pub one_drive: Vec<WinPath>,
}

fn inside_any(path: &WinPath, roots: &[WinPath]) -> bool {
    roots.iter().any(|root| root.contains(path))
}

pub fn classify_folder(
    path: &WinPath,
    roots: &KnownRoots,
    watched: &[WinPath],
) -> (Option<FolderBlock>, Vec<FolderNote>) {
    let drive_root = path.components().is_empty();
    let profile = roots
        .profile
        .as_ref()
        .is_some_and(|profile| profile.key() == path.key());
    let broad = drive_root || profile || inside_any(path, &roots.windows);
    let nested = watched
        .iter()
        .find(|other| other.overlaps(path))
        .map(|other| FolderBlock::Nested {
            name: other.file_name().unwrap_or_default().to_owned(),
        });
    let block = broad.then_some(FolderBlock::TooBroad).or(nested);
    let mut notes = Vec::new();
    if roots.general.iter().any(|root| root.key() == path.key()) {
        notes.push(FolderNote::General);
    }
    if inside_any(path, &roots.one_drive) {
        notes.push(FolderNote::OneDrive);
    }
    if path.is_unc() {
        notes.push(FolderNote::Network);
    }
    (block, notes)
}

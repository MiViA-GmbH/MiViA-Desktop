use mivia_application::settings::Integration;
use mivia_application::setup::SetupDraft;
use mivia_domain::ids::FolderId;
use mivia_ports::ipc::setup::{ArrivalPreset, ConnectionNote, ReportMode, SetupOption, SetupStep};
use mivia_ports::telemetry::{
    ExistingImages, FinishPreset, FinishReportMode, FolderAdded, FolderVia, WizardBack,
    WizardFinished, WizardRestarted, WizardStepCompleted, WizardStepShown,
};

use super::refresh_gate;
use crate::shell::cx::Services;

const fn order(step: SetupStep) -> u8 {
    match step {
        SetupStep::Welcome => 0,
        SetupStep::Legacy => 1,
        SetupStep::Key => 2,
        SetupStep::Folder => 3,
        SetupStep::Series => 4,
        SetupStep::Models => 5,
        SetupStep::Finish => 6,
    }
}

pub(crate) async fn step_changed(services: &Services, from: SetupStep, to: SetupStep) {
    if from == to {
        return;
    }
    let telemetry = &services.telemetry;
    if order(to) < order(from) {
        telemetry.record(
            WizardBack {
                from: Some(from),
                to: Some(to),
            }
            .into(),
        );
    } else {
        telemetry.record(
            WizardStepCompleted {
                state: Some(from),
                duration_ms: None,
            }
            .into(),
        );
    }
    telemetry.record(WizardStepShown { state: Some(to) }.into());
}

pub(crate) fn restarted(services: &Services, step: SetupStep) {
    services.telemetry.record(
        WizardRestarted {
            state: Some(step),
            previous_duration_ms: None,
        }
        .into(),
    );
}

fn existing(folder: Option<FolderId>, draft: &SetupDraft) -> ExistingImages {
    match (folder, draft.has(SetupOption::IncludeExisting)) {
        (None, _) => ExistingImages::None,
        (Some(_), true) => ExistingImages::AlsoExisting,
        (Some(_), false) => ExistingImages::OnlyNew,
    }
}

fn account_connected(services: &Services, draft: &SetupDraft) {
    let imported = draft
        .connection
        .as_ref()
        .is_some_and(|connection| connection.notes.contains(&ConnectionNote::Imported));
    super::account_connected(services, imported);
}

fn folder_added(services: &Services, draft: &SetupDraft, id: FolderId) {
    services.telemetry.record(
        FolderAdded {
            folder_id: Some(id),
            via: Some(FolderVia::Wizard),
            preset: Some(draft.preset),
            report_mode: Some(draft.report_mode),
            existing_images: Some(existing(Some(id), draft)),
            model_choices: Some(draft.models.len() as u64),
            ..FolderAdded::default()
        }
        .into(),
    );
}

const fn finish_choice(draft: &SetupDraft) -> (FinishPreset, FinishReportMode) {
    let preset = match draft.preset {
        ArrivalPreset::Microscope => FinishPreset::Microscope,
        ArrivalPreset::WholeFolder => FinishPreset::WholeFolder,
        ArrivalPreset::ByHand => FinishPreset::ByHand,
    };
    let mode = match draft.report_mode {
        ReportMode::Subfolder => FinishReportMode::Subfolder,
        ReportMode::Beside => FinishReportMode::Beside,
        ReportMode::Custom => FinishReportMode::Custom,
    };
    (preset, mode)
}

pub(crate) async fn finished(
    services: &Services,
    draft: &SetupDraft,
    folder: Option<FolderId>,
    integration: Integration,
) {
    refresh_gate(services).await;
    let (preset, mode) = folder.map_or((FinishPreset::None, FinishReportMode::None), |_| {
        finish_choice(draft)
    });
    services.telemetry.record(
        WizardFinished {
            preset: Some(preset),
            report_mode: Some(mode),
            existing_images: Some(existing(folder, draft)),
            autostart: Some(integration.autostart),
            explorer: Some(integration.explorer),
            skipped_steps: Some(u64::from(draft.has(SetupOption::SkipFolder))),
            model_choices: Some(draft.models.len() as u64),
            duration_ms: None,
        }
        .into(),
    );
    account_connected(services, draft);
    if let Some(id) = folder {
        folder_added(services, draft, id);
    }
}

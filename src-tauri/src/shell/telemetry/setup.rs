use mivia_application::settings::Integration;
use mivia_application::setup::SetupDraft;
use mivia_application::telemetry::{preset_tag, report_mode_tag, step_tag};
use mivia_domain::ids::FolderId;
use mivia_ports::ipc::setup::{ConnectionNote, SetupOption, SetupStep};
use mivia_ports::telemetry::{EventKind, ProductEvent};

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
            ProductEvent::new(EventKind::WizardBack)
                .tag("from", step_tag(from))
                .tag("to", step_tag(to)),
        );
    } else {
        telemetry
            .record(ProductEvent::new(EventKind::WizardStepCompleted).tag("state", step_tag(from)));
    }
    telemetry.record(ProductEvent::new(EventKind::WizardStepShown).tag("state", step_tag(to)));
}

pub(crate) fn restarted(services: &Services, step: SetupStep) {
    services
        .telemetry
        .record(ProductEvent::new(EventKind::WizardRestarted).tag("state", step_tag(step)));
}

fn existing(folder: Option<FolderId>, draft: &SetupDraft) -> &'static str {
    match (folder, draft.has(SetupOption::IncludeExisting)) {
        (None, _) => "none",
        (Some(_), true) => "also_existing",
        (Some(_), false) => "only_new",
    }
}

fn account_connected(services: &Services, draft: &SetupDraft) {
    let imported = draft
        .connection
        .as_ref()
        .is_some_and(|connection| connection.notes.contains(&ConnectionNote::Imported));
    services.telemetry.record(
        ProductEvent::new(EventKind::AccountConnected)
            .tag("kind", "access_key")
            .flag("imported_from_v1", imported),
    );
}

fn folder_added(services: &Services, draft: &SetupDraft, id: FolderId) {
    services.telemetry.record(
        ProductEvent::new(EventKind::FolderAdded)
            .entity("folder_id", "folder", id.0)
            .tag("via", "wizard")
            .tag("preset", preset_tag(draft.preset))
            .tag("report_mode", report_mode_tag(draft.report_mode))
            .tag("existing_images", existing(Some(id), draft))
            .count("model_choices", draft.models.len() as u64),
    );
}

pub(crate) async fn finished(
    services: &Services,
    draft: &SetupDraft,
    folder: Option<FolderId>,
    integration: Integration,
) {
    refresh_gate(services).await;
    let (preset, mode) = folder.map_or(("none", "none"), |_| {
        (preset_tag(draft.preset), report_mode_tag(draft.report_mode))
    });
    services.telemetry.record(
        ProductEvent::new(EventKind::WizardFinished)
            .tag("preset", preset)
            .tag("report_mode", mode)
            .tag("existing_images", existing(folder, draft))
            .flag("autostart", integration.autostart)
            .flag("explorer", integration.explorer)
            .count(
                "skipped_steps",
                u64::from(draft.has(SetupOption::SkipFolder)),
            )
            .count("model_choices", draft.models.len() as u64),
    );
    account_connected(services, draft);
    if let Some(id) = folder {
        folder_added(services, draft, id);
    }
}

use std::path::Path;
use std::sync::Arc;

use mivia_application::settings::global_settings;
use mivia_application::telemetry::{preset_tag, report_mode_tag};
use mivia_domain::ids::FolderId;
use mivia_ports::ipc::folders::NewFolderInput;
use mivia_ports::server::ServerApi;
use mivia_ports::store::AccountScope;
use mivia_ports::telemetry::{EventKind, ProductEvent};

use super::Link;
use crate::shell::cx::Services;

pub(crate) fn folder_added(services: &Services, id: FolderId, input: &NewFolderInput) {
    services.telemetry.record(
        ProductEvent::new(EventKind::FolderAdded)
            .entity("folder_id", "folder", id.0)
            .tag("via", "settings")
            .tag("preset", preset_tag(input.preset))
            .tag("report_mode", report_mode_tag(input.report_mode))
            .count("model_choices", input.models.len() as u64),
    );
}

pub(crate) fn custom_report(services: &Services, reports: usize, series: usize) {
    services.telemetry.record(
        ProductEvent::new(EventKind::ReportCustomCreated)
            .count("count", reports as u64)
            .count("source_batches", series as u64),
    );
}

pub(crate) fn exported(services: &Services, files: &[String]) {
    let sheet = files.iter().any(|file| {
        Path::new(file)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("xlsx"))
    });
    services.telemetry.record(
        ProductEvent::new(EventKind::HistoryExport)
            .count("count", files.len() as u64)
            .tag("format", if sheet { "xlsx" } else { "csv" }),
    );
}

pub(crate) async fn attach_pipeline(
    services: &Services,
    scope: AccountScope,
    api: Arc<dyn ServerApi>,
    pipeline: &Arc<mivia_application::pipeline::Pipeline>,
) {
    let completed = global_settings(services.store.as_ref())
        .await
        .is_ok_and(|settings| settings.setup_completed);
    services.telemetry.attach(Link {
        api,
        scope,
        pipeline: Arc::clone(pipeline),
        completed,
    });
}

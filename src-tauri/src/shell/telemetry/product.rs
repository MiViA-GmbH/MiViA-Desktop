use std::path::Path;
use std::sync::Arc;

use mivia_application::settings::global_settings;
use mivia_domain::ids::FolderId;
use mivia_ports::ipc::folders::NewFolderInput;
use mivia_ports::server::ServerApi;
use mivia_ports::store::AccountScope;
use mivia_ports::telemetry::{
    AccountConnected, AccountDisconnected, AccountKind, AccountState, AccountSwitched,
    ExportFormat, FolderAdded, FolderVia, HistoryExport, ReportCustomCreated,
};

use super::Link;
use crate::shell::cx::Services;

pub(crate) fn folder_added(services: &Services, id: FolderId, input: &NewFolderInput) {
    services.telemetry.record(
        FolderAdded {
            folder_id: Some(id),
            via: Some(FolderVia::Settings),
            preset: Some(input.preset),
            report_mode: Some(input.report_mode),
            model_choices: Some(input.models.len() as u64),
            ..FolderAdded::default()
        }
        .into(),
    );
}

pub(crate) fn account_connected(services: &Services, kind: AccountKind, imported: bool) {
    services.telemetry.record(
        AccountConnected {
            kind: Some(kind),
            imported_from_v1: Some(imported),
        }
        .into(),
    );
}

pub(crate) fn account_switched(services: &Services) {
    let state = Some(AccountState::User);
    services.telemetry.record(AccountSwitched { state }.into());
}

pub(crate) fn account_disconnected(services: &Services) {
    let state = Some(AccountState::User);
    services
        .telemetry
        .record(AccountDisconnected { state }.into());
}

pub(crate) fn custom_report(services: &Services, reports: usize, series: usize) {
    services.telemetry.record(
        ReportCustomCreated {
            count: Some(reports as u64),
            source_batches: Some(series as u64),
        }
        .into(),
    );
}

pub(crate) fn exported(services: &Services, files: &[String]) {
    let sheet = files.iter().any(|file| {
        Path::new(file)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("xlsx"))
    });
    services.telemetry.record(
        HistoryExport {
            count: Some(files.len() as u64),
            format: Some(if sheet {
                ExportFormat::Xlsx
            } else {
                ExportFormat::Csv
            }),
        }
        .into(),
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

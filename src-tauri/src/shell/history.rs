use std::sync::Arc;

use mivia_application::errors::{app_error, history_error};
use mivia_application::history::{CustomRequest, History};
use mivia_application::settings::global_settings;
use mivia_domain::ids::{BatchId, ReportId};
use mivia_domain::naming::Language;
use mivia_domain::output::Format;
use mivia_domain::path::WinPath;
use mivia_os::{KnownFolder, OsFileSystem, SystemClock};
use mivia_ports::clock::Clock;
use mivia_ports::ipc::error::{AppError, AppErrorKind};
use mivia_ports::ipc::history::{CustomReportDraft, CustomReportInput, ReportOrder};
use mivia_ports::ipc::views::OriginView;
use mivia_ports::queries::Queries;
use mivia_ports::store::reports::ReportSpec;
use mivia_ports::store::{StoreError, StoreExt, TxError};
use mivia_ports::telemetry::{EventKind, ProductEvent};

use super::cx::{Cx, Ready, now, parse_id};
use super::folders::known;
use super::setup::language;
use super::telemetry::custom_report;

pub(crate) fn history(ready: &Ready) -> History {
    History {
        fs: Arc::new(OsFileSystem),
        store: Arc::clone(&ready.store) as _,
        queries: Arc::clone(&ready.store) as _,
        scope: ready.scope,
    }
}

pub(crate) fn invalid(code: &'static str, site: &str) -> AppError {
    AppError::new(AppErrorKind::Validation, Some(code), code.to_owned(), site)
}

pub(crate) fn series_ids(ids: &[String], site: &str) -> Result<Vec<BatchId>, AppError> {
    ids.iter()
        .map(|id| parse_id(id, site).map(BatchId))
        .collect()
}

async fn folder_of_series(ready: &Ready, ids: &[BatchId]) -> Option<WinPath> {
    let mut folders = Vec::new();
    for id in ids {
        let view = ready.store.get_batch(ready.scope, *id).await.ok()??;
        let OriginView::Folder { folder_id } = view.summary.origin else {
            return None;
        };
        folders.push(folder_id);
    }
    folders.dedup();
    let [only] = folders.as_slice() else {
        return None;
    };
    let listed = ready.store.list_folders(ready.scope).await.ok()?;
    let folder = listed
        .folders
        .into_iter()
        .find(|folder| &folder.id == only)?;
    WinPath::parse(&folder.report_folder?).ok()
}

async fn default_folder(ready: &Ready, ids: &[BatchId]) -> Option<WinPath> {
    if let Some(folder) = folder_of_series(ready, ids).await {
        return Some(folder);
    }
    let settings = global_settings(ready.store.as_ref()).await.ok()?;
    settings
        .custom_report_folder
        .and_then(|text| WinPath::parse(&text).ok())
        .or_else(|| known(KnownFolder::Profile)?.join("MiViA reports").ok())
}

pub(crate) async fn draft(cx: &Cx, series: &[String]) -> Result<CustomReportDraft, AppError> {
    const SITE: &str = "get_custom_report_draft";
    let ready = cx.ready().await;
    let ids = series_ids(series, SITE)?;
    let mut draft = history(&ready)
        .custom_report_draft(ids.clone())
        .await
        .map_err(|error| history_error(&error, SITE))?;
    draft.language = language(&ready).await.into();
    draft.folder = default_folder(&ready, &ids)
        .await
        .map(|folder| folder.to_string())
        .unwrap_or_default();
    Ok(draft)
}

fn spec(input: &CustomReportInput) -> ReportSpec {
    let language: Language = input.language.into();
    ReportSpec {
        formats: [Format::Pdf, Format::Csv]
            .into_iter()
            .filter(|format| input.formats.iter().any(|code| code == format.code()))
            .collect(),
        language,
        decimal_comma: input.decimal_comma,
        include_images: input.include_images,
        timezone: SystemClock.time_zone(),
    }
}

pub(crate) async fn create(cx: &Cx, input: CustomReportInput) -> Result<u32, AppError> {
    const SITE: &str = "create_custom_report";
    let ready = cx.ready().await;
    let ids = series_ids(&input.series, SITE)?;
    let picked = input
        .folder_pick
        .as_deref()
        .and_then(|id| cx.setup.pick(id));
    let folder = match picked {
        Some(folder) => folder,
        None => default_folder(&ready, &ids)
            .await
            .ok_or_else(|| invalid("noFolder", SITE))?,
    };
    let spec = spec(&input);
    if spec.formats.is_empty() {
        return Err(invalid("noFormat", SITE));
    }
    let request = CustomRequest {
        series: ids,
        name: input.name,
        spec,
        by_date: input.order == ReportOrder::Date,
        folder,
        app_version: ready.services.version.clone(),
        now: now(),
    };
    let created = history(&ready).create_custom_reports(request).await;
    let created = created.map_err(|error| history_error(&error, SITE))?;
    input
        .folder_pick
        .iter()
        .for_each(|id| cx.setup.used_pick(id));
    ready.wake();
    custom_report(&ready.services, created.len(), input.series.len());
    Ok(u32::try_from(created.len()).unwrap_or(u32::MAX))
}

#[derive(Clone, Copy)]
pub(crate) enum ReportAction {
    Update,
    Keep,
    Move,
    Restore,
}

pub(crate) async fn report_action(cx: &Cx, id: &str, action: ReportAction) -> Result<(), AppError> {
    const SITE: &str = "report_action";
    let ready = cx.ready().await;
    let report = ReportId(parse_id(id, SITE)?);
    let history = history(&ready);
    let at = now();
    let done = match action {
        ReportAction::Update => history.update_report(report, at).await.map(drop),
        ReportAction::Keep => history.keep_report(report, at).await,
        ReportAction::Move => history.move_to_replaced(report, at).await,
        ReportAction::Restore => history.restore_from_replaced(report, at).await,
    };
    done.map_err(|error| history_error(&error, SITE))?;
    ready.wake();
    if matches!(action, ReportAction::Update) {
        ready
            .services
            .telemetry
            .record(ProductEvent::new(EventKind::ReportStaleUpdated).entity(
                "report_id",
                "report",
                report.0,
            ));
    }
    Ok(())
}

pub(crate) async fn rename(cx: &Cx, id: &str, label: String) -> Result<(), AppError> {
    const SITE: &str = "rename_series";
    let ready = cx.ready().await;
    let batch = BatchId(parse_id(id, SITE)?);
    let renamed = ready
        .store
        .write(mivia_application::history::rename_series(
            ready.scope,
            batch,
            label,
            now(),
        ))
        .await;
    let outcome = renamed.map_err(|error| match error {
        StoreError::Tx(TxError::Illegal(_)) => AppError::new(
            AppErrorKind::Conflict,
            Some("renameLocked"),
            "rename locked".to_owned(),
            SITE,
        ),
        other @ (StoreError::Tx(_)
        | StoreError::Panicked
        | StoreError::Unavailable
        | StoreError::DiskFull
        | StoreError::Sql(_)) => app_error(&other, SITE),
    });
    if outcome.is_ok() {
        ready.services.telemetry.record(
            ProductEvent::new(EventKind::BatchRenamed).entity("batch_id", "batch", batch.0),
        );
    }
    outcome
}

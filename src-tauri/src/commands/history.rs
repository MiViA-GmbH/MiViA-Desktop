use mivia_application::errors::app_error;
use mivia_domain::ids::BatchId;
use mivia_ports::ipc::error::AppError;
use mivia_ports::ipc::history::{
    BackupsView, CustomReportDraft, CustomReportInput, CustomReportsPage, DeleteHistoryInput,
    DeleteHistoryView, ExportView, HistoryCursor, HistoryEventsPage, HistoryFilter, HistoryPage,
};
use mivia_ports::queries::Queries;
use tauri::State;

use crate::shell::cx::{Cx, parse_id};
use crate::shell::history::ReportAction;
use crate::shell::{backups, export, history};

#[tauri::command]
#[specta::specta]
pub(crate) async fn list_history(
    cx: State<'_, Cx>,
    filter: HistoryFilter,
    after: Option<HistoryCursor>,
) -> Result<HistoryPage, AppError> {
    let ready = cx.ready().await;
    let found = ready.store.list_history(ready.scope, filter, after).await;
    found.map_err(|error| app_error(&error, "list_history"))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn list_custom_reports(
    cx: State<'_, Cx>,
    q: Option<String>,
    after: Option<String>,
) -> Result<CustomReportsPage, AppError> {
    let ready = cx.ready().await;
    let found = ready.store.list_custom_reports(ready.scope, q, after).await;
    found.map_err(|error| app_error(&error, "list_custom_reports"))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn list_batch_events(
    cx: State<'_, Cx>,
    id: String,
    after: Option<String>,
) -> Result<HistoryEventsPage, AppError> {
    let (ready, batch) = (cx.ready().await, parse_id(&id, "list_batch_events")?);
    let found = ready
        .store
        .list_batch_events(ready.scope, BatchId(batch), after)
        .await;
    found.map_err(|error| app_error(&error, "list_batch_events"))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_custom_report_draft(
    cx: State<'_, Cx>,
    series: Vec<String>,
) -> Result<CustomReportDraft, AppError> {
    history::draft(&cx, &series).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn create_custom_report(
    cx: State<'_, Cx>,
    input: CustomReportInput,
) -> Result<u32, AppError> {
    history::create(&cx, input).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn rename_series(
    cx: State<'_, Cx>,
    id: String,
    label: String,
) -> Result<(), AppError> {
    history::rename(&cx, &id, label).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn delete_history(
    cx: State<'_, Cx>,
    input: DeleteHistoryInput,
) -> Result<DeleteHistoryView, AppError> {
    history::delete(&cx, input).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn update_report(cx: State<'_, Cx>, id: String) -> Result<(), AppError> {
    history::report_action(&cx, &id, ReportAction::Update).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn keep_report(cx: State<'_, Cx>, id: String) -> Result<(), AppError> {
    history::report_action(&cx, &id, ReportAction::Keep).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn move_report_to_replaced(cx: State<'_, Cx>, id: String) -> Result<(), AppError> {
    history::report_action(&cx, &id, ReportAction::Move).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn restore_report(cx: State<'_, Cx>, id: String) -> Result<(), AppError> {
    history::report_action(&cx, &id, ReportAction::Restore).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn export_history(
    cx: State<'_, Cx>,
    filter: Option<HistoryFilter>,
    series: Option<String>,
    pick: String,
) -> Result<ExportView, AppError> {
    export::export(&cx, filter, series, &pick).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn list_backups(cx: State<'_, Cx>) -> Result<BackupsView, AppError> {
    backups::view(&cx).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn set_backup_folder(
    cx: State<'_, Cx>,
    pick: Option<String>,
) -> Result<BackupsView, AppError> {
    backups::set_folder(&cx, pick).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn restore_backup(cx: State<'_, Cx>, id: String) -> Result<(), AppError> {
    backups::restore(&cx, &id).await
}

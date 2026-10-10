use mivia_ports::ipc::error::AppError;
use mivia_ports::ipc::support::{
    PendingReportView, ProblemReportInput, ProblemReportPreview, SendOutcome, SupportView,
};
use tauri::State;

use crate::shell::cx::Cx;
use crate::shell::support;

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_support(cx: State<'_, Cx>) -> Result<SupportView, AppError> {
    Ok(support::view(&*cx.ready().await))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn claim_report_offer(
    cx: State<'_, Cx>,
    signature: String,
) -> Result<bool, AppError> {
    let ready = cx.ready().await;
    Ok(support::claim_offer(&cx, &ready, &signature))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn prepare_problem_report(
    cx: State<'_, Cx>,
    input: ProblemReportInput,
) -> Result<ProblemReportPreview, AppError> {
    support::prepare(&cx, input).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn read_problem_report_file(
    cx: State<'_, Cx>,
    report_id: String,
    name: String,
) -> Result<String, AppError> {
    support::read_file(&cx, &report_id, &name)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn send_problem_report(
    cx: State<'_, Cx>,
    report_id: String,
) -> Result<SendOutcome, AppError> {
    support::send(&cx, &report_id).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn save_problem_report(
    cx: State<'_, Cx>,
    report_id: String,
) -> Result<(), AppError> {
    support::save(&cx, &report_id).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn list_pending_reports(
    cx: State<'_, Cx>,
) -> Result<Vec<PendingReportView>, AppError> {
    Ok(support::pending(&*cx.ready().await))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn cancel_pending_report(
    cx: State<'_, Cx>,
    report_id: String,
) -> Result<(), AppError> {
    let ready = cx.ready().await;
    support::cancel(&cx, &ready, &report_id)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn delete_local_logs(cx: State<'_, Cx>) -> Result<u32, AppError> {
    let ready = cx.ready().await;
    Ok(support::delete_logs(&cx, &ready).await)
}

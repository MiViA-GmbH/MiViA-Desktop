use mivia_ports::ipc::error::AppError;
use mivia_ports::ipc::preview::{PreviewResultsView, PreviewTarget, PreviewView};
use tauri::State;

use crate::shell::cx::Cx;
use crate::shell::preview;

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_preview(
    cx: State<'_, Cx>,
    target: PreviewTarget,
) -> Result<PreviewView, AppError> {
    preview::render(&*cx.ready().await, target).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_preview_results(
    cx: State<'_, Cx>,
    computation_id: String,
) -> Result<PreviewResultsView, AppError> {
    preview::results(&*cx.ready().await, &computation_id).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn close_preview(cx: State<'_, Cx>) -> Result<(), AppError> {
    preview::close(&*cx.ready().await)
}

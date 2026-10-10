use mivia_ports::ipc::error::AppError;
use mivia_ports::ipc::privacy::{PrivacyPanel, PrivacyView, SentBatchView};
use tauri::State;

use crate::shell::cx::Cx;
use crate::shell::privacy;

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_privacy(cx: State<'_, Cx>) -> Result<PrivacyView, AppError> {
    Ok(privacy::view(&*cx.ready().await))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn set_telemetry(
    cx: State<'_, Cx>,
    enabled: bool,
) -> Result<PrivacyView, AppError> {
    privacy::set(&cx, enabled).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn list_sent_telemetry(cx: State<'_, Cx>) -> Result<Vec<SentBatchView>, AppError> {
    privacy::sent(&*cx.ready().await).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn note_privacy_viewed(
    cx: State<'_, Cx>,
    panel: PrivacyPanel,
) -> Result<(), AppError> {
    privacy::viewed(&*cx.ready().await, panel);
    Ok(())
}

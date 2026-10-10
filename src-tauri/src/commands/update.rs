use mivia_ports::ipc::error::AppError;
use mivia_ports::ipc::update::{UpdateSettingsInput, UpdateView};
use tauri::State;

use crate::shell::cx::Cx;
use crate::shell::update;

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_update(cx: State<'_, Cx>) -> Result<UpdateView, AppError> {
    update::view(&cx)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn check_for_updates(cx: State<'_, Cx>) -> Result<UpdateView, AppError> {
    update::check(&cx).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn install_update(cx: State<'_, Cx>) -> Result<UpdateView, AppError> {
    update::install(&cx).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn set_update_settings(
    cx: State<'_, Cx>,
    input: UpdateSettingsInput,
) -> Result<UpdateView, AppError> {
    update::set(&cx, input).await
}

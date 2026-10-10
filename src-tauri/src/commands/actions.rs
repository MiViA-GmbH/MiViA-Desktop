use mivia_application::errors::{app_error, pipeline_error};
use mivia_application::settings::{clear_attention, save_language};
use mivia_domain::ids::{BatchId, ReportPartId};
use mivia_ports::ipc::error::AppError;
use mivia_ports::ipc::one_off::OneOffInput;
use mivia_ports::ipc::screens::{LanguageSetting, OpenTarget};
use mivia_ports::store::{AttentionCleared, StoreExt};
use tauri::{AppHandle, State};

use crate::shell::cx::{Cx, now, parse_id};
use crate::shell::{locale, one_off, open, series_actions, window};

#[tauri::command]
#[specta::specta]
pub(crate) async fn ui_ready(app: AppHandle) {
    window::shown(&app);
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn set_language(
    app: AppHandle,
    cx: State<'_, Cx>,
    setting: LanguageSetting,
) -> Result<(), AppError> {
    let ready = cx.ready().await;
    save_language(ready.store.as_ref(), setting)
        .await
        .map_err(|error| app_error(&error, "set_language"))?;
    locale::changed(&app, &ready).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn finish_series(cx: State<'_, Cx>, id: String) -> Result<(), AppError> {
    series_actions::finish_series(&cx, &id).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn check_folder_again(cx: State<'_, Cx>, id: String) -> Result<(), AppError> {
    series_actions::check_folder_again(&cx, &id).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn dismiss_watched_elsewhere(
    cx: State<'_, Cx>,
    id: String,
) -> Result<(), AppError> {
    series_actions::dismiss_watched_elsewhere(&cx, &id).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn pause_folder(cx: State<'_, Cx>, id: String) -> Result<(), AppError> {
    series_actions::pause_folder(&cx, &id).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn resume_folder(cx: State<'_, Cx>, id: String) -> Result<u32, AppError> {
    series_actions::resume_folder(&cx, &id).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn clear_series_attention(
    cx: State<'_, Cx>,
    id: String,
    how: AttentionCleared,
) -> Result<(), AppError> {
    let ready = cx.ready().await;
    let batch = BatchId(parse_id(&id, "clear_series_attention")?);
    ready
        .store
        .write(clear_attention(ready.scope, batch, how, now()))
        .await
        .map_err(|error| app_error(&error, "clear_series_attention"))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn try_again_report_part(cx: State<'_, Cx>, id: String) -> Result<(), AppError> {
    let part = ReportPartId(parse_id(&id, "try_again_report_part")?);
    let retried = cx
        .ready()
        .await
        .pipeline()?
        .try_again_report_part(part)
        .await;
    retried.map_err(|error| pipeline_error(&error, "try_again_report_part"))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn check_connection_now(cx: State<'_, Cx>) -> Result<(), AppError> {
    let probed = cx.ready().await.pipeline()?.probe_now().await;
    probed.map_err(|error| pipeline_error(&error, "check_connection_now"))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn open_target(cx: State<'_, Cx>, target: OpenTarget) -> Result<(), AppError> {
    open::target(&*cx.ready().await, target).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn confirm_one_off(
    cx: State<'_, Cx>,
    input: OneOffInput,
) -> Result<String, AppError> {
    one_off::confirm(&cx, input).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn discard_one_off(cx: State<'_, Cx>, group: Option<u32>) -> Result<(), AppError> {
    one_off::discard(&cx, group).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn add_images(app: AppHandle) -> Result<u32, AppError> {
    one_off::add_images(&app).await
}

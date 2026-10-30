use mivia_ports::ipc::error::AppError;
use mivia_ports::ipc::folders::{CatalogueView, FolderPick, NewFolderInput, PickPurpose};
use mivia_ports::ipc::setup::{ConnectOutcome, DoneView, IntegrationView, SetupInput, SetupView};
use tauri::{AppHandle, State};

use crate::shell::cx::Cx;
use crate::shell::{account, folders, integration, setup, sign_in, switch};

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_setup(cx: State<'_, Cx>) -> Result<SetupView, AppError> {
    setup::view(&cx).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_catalogue(cx: State<'_, Cx>) -> Result<CatalogueView, AppError> {
    setup::catalogue(&cx).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_integration(cx: State<'_, Cx>) -> Result<IntegrationView, AppError> {
    integration::view(&cx).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn save_setup(
    cx: State<'_, Cx>,
    input: SetupInput,
) -> Result<SetupView, AppError> {
    setup::save(&cx, input).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn pick_setup_folder(
    app: AppHandle,
    cx: State<'_, Cx>,
    purpose: PickPurpose,
) -> Result<SetupView, AppError> {
    setup::pick(&cx, &app, purpose).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn connect_access_key(
    cx: State<'_, Cx>,
    key: Option<String>,
) -> Result<ConnectOutcome, AppError> {
    setup::connect(&cx, key).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn start_browser_sign_in(
    app: AppHandle,
    cx: State<'_, Cx>,
    different_account: bool,
) -> Result<ConnectOutcome, AppError> {
    sign_in::browser(&cx, &app, different_account).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn start_code_sign_in(
    app: AppHandle,
    cx: State<'_, Cx>,
) -> Result<ConnectOutcome, AppError> {
    sign_in::code(&cx, &app).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn cancel_sign_in(cx: State<'_, Cx>) -> Result<(), AppError> {
    cx.setup.stop_sign_in();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn import_legacy_key(cx: State<'_, Cx>) -> Result<ConnectOutcome, AppError> {
    setup::import_legacy(&cx).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn close_legacy_app(cx: State<'_, Cx>) -> Result<SetupView, AppError> {
    setup::close_legacy(&cx).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn complete_setup(
    app: AppHandle,
    cx: State<'_, Cx>,
) -> Result<DoneView, AppError> {
    setup::complete(&cx, &app).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn quit_app(app: AppHandle) {
    crate::shell::update::quit(&app).await;
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn pick_folder(
    app: AppHandle,
    cx: State<'_, Cx>,
    purpose: PickPurpose,
) -> Result<Option<FolderPick>, AppError> {
    folders::pick_new(&cx, &app, purpose).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn add_folder(
    cx: State<'_, Cx>,
    input: NewFolderInput,
) -> Result<String, AppError> {
    folders::add(&cx, input).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn relocate_folder(
    cx: State<'_, Cx>,
    id: String,
    folder_pick: String,
) -> Result<(), AppError> {
    folders::relocate(&cx, &id, &folder_pick).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn switch_account(
    app: AppHandle,
    cx: State<'_, Cx>,
) -> Result<ConnectOutcome, AppError> {
    switch::switch(&cx, &app).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn sign_out(cx: State<'_, Cx>) -> Result<(), AppError> {
    account::sign_out(&cx).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn set_integration(
    cx: State<'_, Cx>,
    autostart: Option<bool>,
    explorer: Option<bool>,
) -> Result<IntegrationView, AppError> {
    integration::set(&cx, autostart, explorer).await
}

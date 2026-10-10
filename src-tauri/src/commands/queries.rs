use mivia_application::errors::app_error;
use mivia_domain::ids::BatchId;
use mivia_ports::ipc::error::AppError;
use mivia_ports::ipc::screens::{AccountView, HoldsView, LocaleView, SeriesReportsView};
use mivia_ports::ipc::views::{
    BatchItemsPage, BatchView, FoldersView, HistoryCursor, HistoryPage, NeedsAttentionView,
};
use mivia_ports::queries::Queries;
use tauri::State;

use crate::shell::cx::{Cx, parse_id};
use crate::shell::locale;

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_locale(cx: State<'_, Cx>) -> Result<LocaleView, AppError> {
    locale::current(&*cx.ready().await).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_account(cx: State<'_, Cx>) -> Result<Option<AccountView>, AppError> {
    let ready = cx.ready().await;
    let found = ready.store.get_account(ready.scope).await;
    found.map_err(|error| app_error(&error, "get_account"))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn list_folders(cx: State<'_, Cx>) -> Result<FoldersView, AppError> {
    let ready = cx.ready().await;
    let found = ready.store.list_folders(ready.scope).await;
    found.map_err(|error| app_error(&error, "list_folders"))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn list_holds(cx: State<'_, Cx>) -> Result<HoldsView, AppError> {
    let ready = cx.ready().await;
    let found = ready.store.list_holds(ready.scope).await;
    found.map_err(|error| app_error(&error, "list_holds"))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn list_needs_attention(
    cx: State<'_, Cx>,
) -> Result<NeedsAttentionView, AppError> {
    let ready = cx.ready().await;
    let found = ready.store.list_needs_attention(ready.scope).await;
    found.map_err(|error| app_error(&error, "list_needs_attention"))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn list_history(
    cx: State<'_, Cx>,
    after: Option<HistoryCursor>,
) -> Result<HistoryPage, AppError> {
    let ready = cx.ready().await;
    let found = ready.store.list_history(ready.scope, after).await;
    found.map_err(|error| app_error(&error, "list_history"))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_batch(
    cx: State<'_, Cx>,
    id: String,
) -> Result<Option<BatchView>, AppError> {
    let ready = cx.ready().await;
    let found = ready
        .store
        .get_batch(ready.scope, BatchId(parse_id(&id, "get_batch")?));
    found.await.map_err(|error| app_error(&error, "get_batch"))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn list_batch_items(
    cx: State<'_, Cx>,
    id: String,
    after: Option<u32>,
) -> Result<BatchItemsPage, AppError> {
    let ready = cx.ready().await;
    let batch = ready.store.list_batch_items(
        ready.scope,
        BatchId(parse_id(&id, "list_batch_items")?),
        after,
    );
    batch
        .await
        .map_err(|error| app_error(&error, "list_batch_items"))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn list_series_reports(
    cx: State<'_, Cx>,
    id: String,
) -> Result<SeriesReportsView, AppError> {
    let ready = cx.ready().await;
    let found = ready
        .store
        .list_series_reports(ready.scope, BatchId(parse_id(&id, "list_series_reports")?));
    found
        .await
        .map_err(|error| app_error(&error, "list_series_reports"))
}

use mivia_ports::ipc::error::AppError;
use mivia_ports::ipc::network::{NetworkView, ProxySetting};
use tauri::State;

use crate::shell::cx::Cx;
use crate::shell::network;

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_network(cx: State<'_, Cx>) -> Result<NetworkView, AppError> {
    Ok(network::view(&*cx.ready().await).await)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn get_network_details(cx: State<'_, Cx>) -> Result<String, AppError> {
    Ok(network::details(&*cx.ready().await).await)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn set_proxy_setting(
    cx: State<'_, Cx>,
    setting: ProxySetting,
) -> Result<NetworkView, AppError> {
    network::set(&cx, setting).await
}

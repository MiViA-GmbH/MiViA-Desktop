use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tauri::utils::config::Color;
use tauri::webview::{NewWindowResponse, PermissionResponse};
use tauri::{AppHandle, Manager, Url, WebviewWindow, WebviewWindowBuilder, Window, WindowEvent};
use tauri_specta::Event;

use super::cx::Cx;
use super::events::{Navigate, Route};

pub(crate) const LABEL: &str = "main";
const HIDDEN_CHECKS: u32 = 10;
const HIDDEN_CHECK_EVERY: Duration = Duration::from_secs(60);
const STONE_1: Color = Color(0xf0, 0xf0, 0xf0, 0xff);
const NO_PEER_CONNECTIONS: &str = "delete window.RTCPeerConnection;\
     delete window.webkitRTCPeerConnection;delete window.RTCDataChannel;";

#[cfg(feature = "e2e")]
const WRY_DEFAULT_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";

static GENERATION: AtomicU64 = AtomicU64::new(0);

fn app_origin(url: &Url) -> bool {
    let local = matches!(url.host_str(), Some("tauri.localhost"));
    let dev =
        tauri::is_dev() && matches!(url.host_str(), Some("localhost")) && url.port() == Some(5173);
    url.scheme() == "http" && (local || dev)
}

fn set_visible(window: &WebviewWindow, visible: bool) {
    let applied = window.with_webview(move |webview| {
        if !mivia_os::set_webview_visible(&webview.controller(), visible) {
            tracing::debug!(visible, "window.memoryLevelFailed");
        }
    });
    if let Err(error) = applied {
        tracing::debug!(%error, "window.memoryLevelFailed");
    }
}

fn build(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == LABEL)
        .cloned()
        .unwrap_or_default();
    let data = app.state::<Cx>().data_dir.join("EBWebView");
    let builder = WebviewWindowBuilder::from_config(app, &config)?;
    #[cfg(feature = "e2e")]
    let builder = match super::e2e::cdp_port() {
        Some(port) => builder.additional_browser_args(&format!(
            "{WRY_DEFAULT_ARGS} --remote-debugging-port={port}"
        )),
        None => builder,
    };
    builder
        .data_directory(data)
        .background_color(STONE_1)
        .on_navigation(app_origin)
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .on_download(|_, _| false)
        .on_permission_request(|_, _| PermissionResponse::Deny)
        .general_autofill_enabled(false)
        .initialization_script_for_all_frames(NO_PEER_CONNECTIONS)
        .build()
}

pub(crate) fn open(app: &AppHandle) {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    super::mark("window_open_requested");
    let Some(window) = app.get_webview_window(LABEL) else {
        if let Err(error) = build(app) {
            tracing::error!(%error, "window.buildFailed");
        }
        return;
    };
    set_visible(&window, true);
    show(&window);
    if let Err(error) = (Navigate {
        route: Route::Overview,
    })
    .emit(app)
    {
        tracing::warn!(%error, "ipc.emitFailed");
    }
}

fn show(window: &WebviewWindow) {
    if let Err(error) = window.show().and_then(|()| window.set_focus()) {
        tracing::warn!(%error, "window.showFailed");
    }
}

pub(crate) fn shown(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        show(&window);
        super::mark("shown_painted");
    }
}

pub(crate) fn on_event(window: &Window, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        hide(window.app_handle());
    }
    if matches!(event, WindowEvent::Destroyed) {
        if let Some(ready) = window.app_handle().state::<Cx>().get() {
            let store = std::sync::Arc::clone(&ready.store);
            tauri::async_runtime::spawn(async move { store.close_readers().await });
        }
        tracing::info!("window.destroyed");
    }
}

fn hide(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    if let Err(error) = window.hide() {
        tracing::warn!(%error, "window.hideFailed");
    }
    set_visible(&window, false);
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    tauri::async_runtime::spawn(destroy_later(app.clone(), generation));
}

async fn destroy_later(app: AppHandle, generation: u64) {
    let current = || GENERATION.load(Ordering::SeqCst) == generation;
    for _ in 0..HIDDEN_CHECKS {
        tokio::time::sleep(HIDDEN_CHECK_EVERY).await;
        if !current() {
            return;
        }
        if mivia_os::memory_is_low() {
            break;
        }
    }
    if let Some(window) = app.get_webview_window(LABEL).filter(|_| current())
        && let Err(error) = window.destroy()
    {
        tracing::warn!(%error, "window.destroyFailed");
    }
}

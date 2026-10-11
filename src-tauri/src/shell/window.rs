use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use mivia_domain::path::WinPath;
use tauri::utils::config::Color;
use tauri::webview::{NewWindowResponse, PermissionResponse};
use tauri::{
    AppHandle, DragDropEvent, Manager, Url, WebviewWindow, WebviewWindowBuilder, Window,
    WindowEvent,
};
use tauri_specta::Event;

use super::cx::Cx;
use super::events::{DropHover, Navigate, QuitRequested, Route};
use super::one_off;

pub(crate) const LABEL: &str = "main";
const HIDDEN_CHECKS: u32 = 10;
const HIDDEN_CHECKS_AFTER_PREVIEW: u32 = 2;
const HIDDEN_CHECK_EVERY: Duration = Duration::from_secs(60);
const STONE_1: Color = Color(0xf0, 0xf0, 0xf0, 0xff);
const NO_PEER_CONNECTIONS: &str = "delete window.RTCPeerConnection;\
     delete window.webkitRTCPeerConnection;delete window.RTCDataChannel;";

#[cfg(feature = "e2e")]
const WRY_DEFAULT_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";

static GENERATION: AtomicU64 = AtomicU64::new(0);
static REPORT_PENDING: AtomicBool = AtomicBool::new(false);
static PAINTED: AtomicBool = AtomicBool::new(false);
static PREVIEW_SHOWN: AtomicBool = AtomicBool::new(false);

pub(crate) fn preview_shown() {
    PREVIEW_SHOWN.store(true, Ordering::SeqCst);
}

fn app_origin(url: &Url) -> bool {
    let local = matches!(url.host_str(), Some("tauri.localhost"));
    let dev =
        tauri::is_dev() && matches!(url.host_str(), Some("localhost")) && url.port() == Some(5173);
    url.scheme() == "http" && (local || dev)
}

fn set_visible(window: &WebviewWindow, visible: bool) {
    let applied = window.with_webview(move |webview| {
        let controller = webview.controller();
        if !mivia_os::set_webview_visible(&controller, visible) {
            tracing::debug!(visible, "window.memoryLevelFailed");
        }
        if !visible && suspend_hidden() && !mivia_os::suspend_webview(&controller) {
            tracing::debug!("window.suspendFailed");
        }
    });
    if let Err(error) = applied {
        tracing::debug!(%error, "window.memoryLevelFailed");
    }
}

fn suspend_hidden() -> bool {
    #[cfg(feature = "e2e")]
    {
        !super::e2e::no_suspend()
    }
    #[cfg(not(feature = "e2e"))]
    {
        true
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

pub(crate) fn reveal(app: &AppHandle) {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    super::mark("window_open_requested");
    let Some(window) = app.get_webview_window(LABEL) else {
        PAINTED.store(false, Ordering::SeqCst);
        match build(app) {
            Ok(_) => super::mark("build_done"),
            Err(error) => tracing::error!(%error, "window.buildFailed"),
        }
        return;
    };
    set_visible(&window, true);
    show(&window);
}

pub(crate) fn open(app: &AppHandle) {
    open_at(app, Route::Overview);
}

pub(crate) fn open_at(app: &AppHandle, route: Route) {
    let existed = app.get_webview_window(LABEL).is_some();
    reveal(app);
    if existed {
        navigate(app, route);
    } else {
        REPORT_PENDING.store(matches!(route, Route::ProblemReport), Ordering::SeqCst);
    }
}

fn navigate(app: &AppHandle, route: Route) {
    if let Err(error) = (Navigate { route }).emit(app) {
        tracing::warn!(%error, "ipc.emitFailed");
    }
}

fn show(window: &WebviewWindow) {
    if let Err(error) = window.show().and_then(|()| window.set_focus()) {
        tracing::warn!(%error, "window.showFailed");
    }
}

pub(crate) fn shown(app: &AppHandle) {
    if let Some(ready) = app.state::<Cx>().get() {
        ready.store.keep_readers();
    }
    if let Some(window) = app.get_webview_window(LABEL) {
        show(&window);
        super::mark(if PAINTED.swap(true, Ordering::SeqCst) {
            "shown_painted"
        } else {
            "ui_ready"
        });
    }
    if REPORT_PENDING.swap(false, Ordering::SeqCst) {
        navigate(app, Route::ProblemReport);
    }
}

fn dropped(app: &AppHandle, event: &DragDropEvent) {
    if matches!(event, DragDropEvent::Over { .. }) {
        return;
    }
    let over = matches!(event, DragDropEvent::Enter { .. });
    if let Err(error) = DropHover(over).emit(app) {
        tracing::warn!(%error, "ipc.emitFailed");
    }
    let DragDropEvent::Drop { paths, .. } = event else {
        return;
    };
    let paths: Vec<WinPath> = paths
        .iter()
        .filter_map(|path| WinPath::parse(path.to_str()?).ok())
        .collect();
    let app = app.clone();
    tauri::async_runtime::spawn(async move { one_off::stage(&app, None, paths).await });
}

pub(crate) fn on_event(window: &Window, event: &WindowEvent) {
    if let WindowEvent::DragDrop(drop) = event {
        dropped(window.app_handle(), drop);
    }
    if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let app = window.app_handle();
        if app.state::<Cx>().completed() {
            hide(app);
        } else if let Err(error) = QuitRequested.emit(app) {
            tracing::warn!(%error, "ipc.emitFailed");
        }
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
    let checks = if PREVIEW_SHOWN.swap(false, Ordering::SeqCst) {
        HIDDEN_CHECKS_AFTER_PREVIEW
    } else {
        HIDDEN_CHECKS
    };
    tauri::async_runtime::spawn(destroy_later(app.clone(), generation, checks));
}

async fn destroy_later(app: AppHandle, generation: u64, checks: u32) {
    let current = || GENERATION.load(Ordering::SeqCst) == generation;
    for _ in 0..checks {
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

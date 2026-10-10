use tauri::menu::{Menu, MenuEvent, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use super::window;

const OPEN: &str = "open";
const QUIT: &str = "quit";

struct Starting {
    menu: Menu<Wry>,
    item: MenuItem<Wry>,
}

pub(super) fn build(app: &AppHandle) -> tauri::Result<()> {
    let starting = MenuItem::with_id(app, "starting", "Starting...", false, None::<&str>)?;
    let open = MenuItem::with_id(app, OPEN, "Open MiViA Desktop", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT, "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&starting, &open, &quit])?;
    app.manage(Starting {
        menu: menu.clone(),
        item: starting,
    });
    let tray = TrayIconBuilder::with_id("main")
        .tooltip("MiViA Desktop")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, &event))
        .on_tray_icon_event(|tray, event| on_click(tray.app_handle(), &event));
    let tray = match app.default_window_icon() {
        Some(icon) => tray.icon(icon.clone()),
        None => tray,
    };
    tray.build(app).map(drop)
}

pub(super) fn started(app: &AppHandle) {
    let starting = app.state::<Starting>();
    if let Err(error) = starting.menu.remove(&starting.item) {
        tracing::warn!(%error, "tray.menuFailed");
    }
}

fn on_click(app: &AppHandle, event: &TrayIconEvent) {
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    {
        window::open(app);
    }
}

fn on_menu(app: &AppHandle, event: &MenuEvent) {
    match event.id().as_ref() {
        OPEN => window::open(app),
        QUIT => app.exit(0),
        _ => {}
    }
}

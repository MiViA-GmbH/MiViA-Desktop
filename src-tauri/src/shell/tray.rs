use tauri::AppHandle;
use tauri::menu::{Menu, MenuEvent, MenuItem};
use tauri::tray::TrayIconBuilder;

const QUIT: &str = "quit";

pub(super) fn build(app: &AppHandle) -> tauri::Result<()> {
    let starting = MenuItem::with_id(app, "starting", "Starting...", false, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT, "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&starting, &quit])?;
    let tray = TrayIconBuilder::with_id("main")
        .tooltip("MiViA Desktop: starting")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, &event));
    let tray = match app.default_window_icon() {
        Some(icon) => tray.icon(icon.clone()),
        None => tray,
    };
    tray.build(app).map(drop)
}

fn on_menu(app: &AppHandle, event: &MenuEvent) {
    if event.id() == QUIT {
        app.exit(0);
    }
}

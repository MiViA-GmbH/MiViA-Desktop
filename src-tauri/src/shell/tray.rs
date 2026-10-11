use mivia_ports::queries::Queries;
use parking_lot::Mutex;
use tauri::menu::{Menu, MenuEvent, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use super::cx::Cx;
use super::events::Route;
use super::{update, window};

mod icon;

use icon::Mark;

const OPEN: &str = "open";
const QUIT: &str = "quit";
const REPORT: &str = "report";
const UPDATES: &str = "updates";

struct Starting {
    menu: Menu<Wry>,
    item: MenuItem<Wry>,
    report: MenuItem<Wry>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Look {
    light: bool,
    mark: Mark,
}

fn render(look: Look) -> Option<tauri::image::Image<'static>> {
    icon::image(look.light, look.mark, mivia_os::small_icon_size())
}

pub(super) fn build(app: &AppHandle) -> tauri::Result<()> {
    let starting = MenuItem::with_id(app, "starting", "Starting...", false, None::<&str>)?;
    let open = MenuItem::with_id(app, OPEN, "Open MiViA Desktop", true, None::<&str>)?;
    let report = MenuItem::with_id(app, REPORT, "Report a problem...", true, None::<&str>)?;
    let updates = MenuItem::with_id(app, UPDATES, "Check for updates", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT, "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&starting, &open, &report, &updates, &quit])?;
    app.manage(Starting {
        menu: menu.clone(),
        item: starting,
        report,
    });
    let look = Look {
        light: mivia_os::taskbar_is_light(),
        mark: Mark::Idle,
    };
    app.manage(Mutex::new(look));
    let tray = TrayIconBuilder::with_id("main")
        .tooltip(look.mark.tooltip())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, &event))
        .on_tray_icon_event(|tray, event| on_click(tray.app_handle(), &event));
    let tray = match render(look).or_else(|| app.default_window_icon().cloned()) {
        Some(icon) => tray.icon(icon),
        None => tray,
    };
    tray.build(app).map(drop)
}

fn visible(app: &AppHandle, visible: bool) {
    if let Some(tray) = app.tray_by_id("main")
        && let Err(error) = tray.set_visible(visible)
    {
        tracing::warn!(%error, "tray.visibilityFailed");
    }
}

pub(super) fn show(app: &AppHandle) {
    visible(app, true);
}

pub(super) fn hide(app: &AppHandle) {
    visible(app, false);
}

fn restyle(app: &AppHandle, change: impl FnOnce(&mut Look)) {
    let state = app.state::<Mutex<Look>>();
    let mut look = state.lock();
    let before = *look;
    change(&mut look);
    if *look == before {
        return;
    }
    let Some(tray) = app.tray_by_id("main") else {
        return;
    };
    if let Some(image) = render(*look)
        && let Err(error) = tray.set_icon(Some(image))
    {
        tracing::warn!(%error, "tray.iconFailed");
    }
    if let Err(error) = tray.set_tooltip(Some(look.mark.tooltip())) {
        tracing::warn!(%error, "tray.tooltipFailed");
    }
}

pub(super) fn theme_changed(app: &AppHandle) {
    let light = mivia_os::taskbar_is_light();
    restyle(app, |look| look.light = light);
}

pub(super) async fn refresh(app: &AppHandle) {
    let Some(ready) = app.state::<Cx>().get() else {
        return;
    };
    let held = ready
        .store
        .list_holds(ready.scope)
        .await
        .is_ok_and(|view| !view.holds.is_empty());
    let attention = held
        || ready
            .store
            .list_needs_attention(ready.scope)
            .await
            .is_ok_and(|view| view.total > 0);
    let working = match &ready.pipeline {
        Some(pipeline) => pipeline.working().await.unwrap_or(false),
        None => false,
    };
    restyle(app, |look| {
        look.mark = Mark::working(working).raised(attention);
    });
}

pub(super) fn started(app: &AppHandle, reporting: bool) {
    let starting = app.state::<Starting>();
    let removed = starting.menu.remove(&starting.item).and_then(|()| {
        if reporting {
            Ok(())
        } else {
            starting.menu.remove(&starting.report)
        }
    });
    if let Err(error) = removed {
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
        REPORT => window::open_at(app, Route::ProblemReport),
        UPDATES => check_updates(app),
        QUIT => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move { update::quit(&app).await });
        }
        _ => {}
    }
}

fn check_updates(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        window::open_at(&app, Route::Updates);
        if let Err(error) = update::check(&app.state::<Cx>()).await {
            tracing::warn!(?error, "tray.updateCheckFailed");
        }
    });
}

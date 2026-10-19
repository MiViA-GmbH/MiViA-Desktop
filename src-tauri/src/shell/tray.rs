use mivia_domain::naming::Language;
use mivia_ports::queries::Queries;
use parking_lot::Mutex;
use tauri::menu::{Menu, MenuEvent, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use super::cx::Cx;
use super::events::Route;
use super::text::{t, text};
use super::{locale, update, window};

mod icon;

use icon::Mark;

const STARTING: &str = "starting";
const OPEN: &str = "open";
const QUIT: &str = "quit";
const REPORT: &str = "report";
const RESTART: &str = "restart";
const UPDATES: &str = "updates";

struct Tray {
    menu: Menu<Wry>,
    items: [MenuItem<Wry>; 6],
    language: Mutex<Language>,
    notice: Mutex<Option<String>>,
}

fn labels(language: Language, version: &str) -> [String; 6] {
    [
        t!("desktop.native.tray.starting", language),
        t!("desktop.native.tray.open", language),
        t!("desktop.native.tray.report", language),
        t!("desktop.native.tray.restart", language).replace("{{version}}", version),
        text(language, "desktop.update.check"),
        t!("desktop.native.tray.quit", language),
    ]
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
    let language = locale::system_language();
    let [starting, open, report, restart, updates, quit] = labels(language, "");
    let item =
        |id: &str, text: String| MenuItem::with_id(app, id, text, id != STARTING, None::<&str>);
    let items = [
        item(STARTING, starting)?,
        item(OPEN, open)?,
        item(REPORT, report)?,
        item(RESTART, restart)?,
        item(UPDATES, updates)?,
        item(QUIT, quit)?,
    ];
    let [starting, open, report, _, updates, quit] = &items;
    let menu = Menu::with_items(app, &[starting, open, report, updates, quit])?;
    app.manage(Tray {
        menu: menu.clone(),
        items,
        language: Mutex::new(language),
        notice: Mutex::new(None),
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

pub(super) fn started(app: &AppHandle, reporting: bool, language: Language) {
    let tray = app.state::<Tray>();
    let [starting, _, report, ..] = &tray.items;
    let removed = tray.menu.remove(starting).and_then(|()| {
        if reporting {
            Ok(())
        } else {
            tray.menu.remove(report)
        }
    });
    if let Err(error) = removed {
        tracing::warn!(%error, "tray.menuFailed");
    }
    relabel(app, language);
}

pub(super) fn relabel(app: &AppHandle, language: Language) {
    let tray = app.state::<Tray>();
    *tray.language.lock() = language;
    let version = tray.notice.lock().clone().unwrap_or_default();
    for (item, label) in tray.items.iter().zip(labels(language, &version)) {
        if let Err(error) = item.set_text(label) {
            tracing::warn!(%error, "tray.menuFailed");
        }
    }
}

pub(super) fn notice(app: &AppHandle, version: Option<&str>) {
    let tray = app.state::<Tray>();
    tray.notice.lock().clone_from(&version.map(str::to_owned));
    let [.., restart, _, _] = &tray.items;
    let shown = tray.menu.get(RESTART).is_some();
    let changed = match version {
        Some(version) => {
            let language = *tray.language.lock();
            let [.., label, _, _] = labels(language, version);
            restart.set_text(label).and_then(|()| {
                let at = tray
                    .menu
                    .items()?
                    .iter()
                    .position(|item| item.id() == UPDATES);
                match at {
                    Some(at) if !shown => tray.menu.insert(restart, at),
                    _ => Ok(()),
                }
            })
        }
        None if shown => tray.menu.remove(restart),
        None => Ok(()),
    };
    if let Err(error) = changed {
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
        RESTART => install_update(app),
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

fn install_update(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = update::install(&app.state::<Cx>()).await {
            tracing::warn!(?error, "tray.installFailed");
        }
    });
}

#[cfg(test)]
mod tests {
    use mivia_domain::naming::Language;

    use super::labels;

    #[test]
    fn every_menu_text_is_found_in_both_languages() {
        let en = labels(Language::En, "2.1.0");
        let de = labels(Language::De, "2.1.0");
        for label in en.iter().chain(&de) {
            assert!(!label.starts_with("desktop."), "{label} is missing");
        }
        assert_eq!(en[3], "Restart and update to 2.1.0");
        assert_eq!(de[3], "Neu starten und auf 2.1.0 aktualisieren");
        assert_eq!(de[4], "Nach Updates suchen");
    }
}

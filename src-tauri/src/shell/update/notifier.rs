use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use mivia_domain::naming::Language;
use mivia_os::{ShellPlaces, Toast};
use mivia_ports::telemetry::{NotificationChannel, NotificationClicked, NotificationKind};
use mivia_ports::update::{Delivery, UpdateNotifier};
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use crate::shell::cx::Services;
use crate::shell::events::{Route, UpdateChanged, UpdateNotice};
use crate::shell::text::{t, text};
use crate::shell::{locale, open, tray, window};

const ICON: &[u8] = include_bytes!("../../../icons/icon.ico");

pub(super) fn register(data_dir: &Path) {
    if cfg!(feature = "e2e") {
        return;
    }
    let icon = data_dir.join("icon.ico");
    let registered = ShellPlaces::user()
        .ok_or_else(|| std::io::Error::other("no SendTo folder"))
        .and_then(|places| {
            if std::fs::read(&icon).ok().as_deref() != Some(ICON) {
                std::fs::write(&icon, ICON)?;
            }
            mivia_os::register_app_id(&places, "MiViA Desktop", &icon)
        });
    if let Err(error) = registered {
        tracing::warn!(%error, "update.appIdFailed");
    }
}

pub(super) fn available_text(language: Language, version: &str, ready: bool) -> [String; 2] {
    let body = if ready {
        text(language, "desktop.update.ready")
    } else {
        text(language, "desktop.update.available")
    };
    [
        text(language, "desktop.update.notice"),
        body.replace("{{version}}", version),
    ]
}

pub(super) fn applied_text(language: Language, version: &str) -> [String; 2] {
    [
        t!("desktop.native.update.applied", language).replace("{{version}}", version),
        t!("desktop.native.update.notes", language),
    ]
}

pub(super) struct ShellNotifier {
    app: AppHandle,
    services: Arc<Services>,
}

impl ShellNotifier {
    pub(super) fn new(app: &AppHandle, services: &Arc<Services>) -> Self {
        Self {
            app: app.clone(),
            services: Arc::clone(services),
        }
    }

    fn clicked(&self) -> impl Fn() + Send + 'static {
        let services = Arc::clone(&self.services);
        move || {
            services.telemetry.record(
                NotificationClicked {
                    kind: Some(NotificationKind::Update),
                    channel: Some(NotificationChannel::Os),
                }
                .into(),
            );
        }
    }

    async fn language(&self) -> Language {
        locale::language(self.services.store.as_ref()).await
    }
}

fn show(toast: &Toast<'_>, on_click: impl Fn() + Send + 'static) -> Delivery {
    match mivia_os::show_toast(toast, on_click) {
        Ok(true) => Delivery::Os,
        Ok(false) => {
            tracing::info!("update.toastsOff");
            Delivery::Off
        }
        Err(error) => {
            tracing::warn!(%error, "update.toastFailed");
            Delivery::Off
        }
    }
}

#[async_trait]
impl UpdateNotifier for ShellNotifier {
    async fn notice_changed(&self, version: Option<&str>) {
        tray::notice(&self.app, version);
        if let Err(error) = UpdateChanged.emit(&self.app) {
            tracing::warn!(%error, "ipc.emitFailed");
        }
    }

    async fn show_available(&self, version: &str, ready: bool) -> Delivery {
        let focused = self
            .app
            .get_webview_window(window::LABEL)
            .is_some_and(|window| window.is_focused().unwrap_or(false));
        if focused {
            let notice = UpdateNotice {
                version: version.to_owned(),
                ready,
            };
            return match notice.emit(&self.app) {
                Ok(()) => Delivery::InApp,
                Err(error) => {
                    tracing::warn!(%error, "ipc.emitFailed");
                    Delivery::Off
                }
            };
        }
        let [title, body] = available_text(self.language().await, version, ready);
        let toast = Toast {
            tag: version,
            title: &title,
            body: Some(&body),
            button: None,
        };
        let clicked = self.clicked();
        let app = self.app.clone();
        show(&toast, move || {
            clicked();
            let app = app.clone();
            tauri::async_runtime::spawn(async move { window::open_at(&app, Route::Updates) });
        })
    }

    async fn show_applied(&self, version: &str) -> Delivery {
        let [title, notes] = applied_text(self.language().await, version);
        let toast = Toast {
            tag: version,
            title: &title,
            body: None,
            button: Some(&notes),
        };
        let clicked = self.clicked();
        show(&toast, move || {
            clicked();
            if let Err(error) = mivia_os::open_file(open::RELEASES) {
                tracing::warn!(%error, "update.notesFailed");
            }
        })
    }

    fn withdraw(&self) {
        if let Err(error) = mivia_os::withdraw_toasts() {
            tracing::debug!(%error, "update.withdrawFailed");
        }
    }
}

#[cfg(test)]
mod tests {
    use mivia_domain::naming::Language;

    use super::{applied_text, available_text};

    #[test]
    fn every_toast_text_is_found_in_both_languages() {
        for language in [Language::En, Language::De] {
            let texts = [
                available_text(language, "2.1.0", true),
                available_text(language, "2.1.0", false),
                applied_text(language, "2.1.0"),
            ];
            for text in texts.iter().flatten() {
                assert!(!text.starts_with("desktop."), "{text} is missing");
                assert!(!text.contains("{{"), "{text} keeps a placeholder");
            }
        }
        assert_eq!(
            available_text(Language::De, "2.1.0", true),
            [
                "Update verfügbar",
                "Version 2.1.0 ist bereit zur Installation."
            ]
        );
        assert_eq!(
            applied_text(Language::En, "2.1.0"),
            ["Updated to 2.1.0.", "Release notes"]
        );
    }
}

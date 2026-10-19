use mivia_application::errors::app_error;
use mivia_application::settings::{global_settings, locale};
use mivia_domain::naming::Language;
use mivia_ports::ipc::error::AppError;
use mivia_ports::ipc::screens::{self, LanguageSetting, LocaleView};
use mivia_ports::queries::Queries;
use tauri::AppHandle;
use tauri_specta::Event;

use super::cx::Ready;
use super::events::LanguageChanged;

pub(crate) async fn current(ready: &Ready) -> Result<LocaleView, AppError> {
    of(ready.store.as_ref()).await
}

async fn of(store: &dyn Queries) -> Result<LocaleView, AppError> {
    let settings = global_settings(store)
        .await
        .map_err(|error| app_error(&error, "get_locale"))?;
    let system = mivia_os::system_locale();
    Ok(locale(
        settings.language,
        system.display_language.as_deref(),
        system.user_locale.as_deref(),
    ))
}

const fn domain(language: screens::Language) -> Language {
    match language {
        screens::Language::De => Language::De,
        screens::Language::En => Language::En,
    }
}

pub(crate) async fn language(store: &dyn Queries) -> Language {
    of(store)
        .await
        .map_or(Language::En, |view| domain(view.language))
}

pub(crate) fn system_language() -> Language {
    let system = mivia_os::system_locale();
    domain(
        locale(
            LanguageSetting::Auto,
            system.display_language.as_deref(),
            None,
        )
        .language,
    )
}

pub(crate) async fn changed(app: &AppHandle, ready: &Ready) -> Result<(), AppError> {
    let view = current(ready).await?;
    super::tray::relabel(app, language(ready.store.as_ref()).await);
    super::integration::heal(&ready.services, super::setup::language(ready).await).await;
    if let Err(error) = LanguageChanged(view).emit(app) {
        tracing::warn!(%error, "ipc.emitFailed");
    }
    Ok(())
}

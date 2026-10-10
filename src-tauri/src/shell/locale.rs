use mivia_application::errors::app_error;
use mivia_application::settings::{global_settings, locale};
use mivia_ports::ipc::error::AppError;
use mivia_ports::ipc::screens::LocaleView;
use tauri::AppHandle;
use tauri_specta::Event;

use super::cx::Ready;
use super::events::LanguageChanged;

pub(crate) async fn current(ready: &Ready) -> Result<LocaleView, AppError> {
    let settings = global_settings(ready.store.as_ref())
        .await
        .map_err(|error| app_error(&error, "get_locale"))?;
    let system = mivia_os::system_locale();
    Ok(locale(
        settings.language,
        system.display_language.as_deref(),
        system.user_locale.as_deref(),
    ))
}

pub(crate) async fn changed(app: &AppHandle, ready: &Ready) -> Result<(), AppError> {
    let view = current(ready).await?;
    if let Err(error) = LanguageChanged(view).emit(app) {
        tracing::warn!(%error, "ipc.emitFailed");
    }
    Ok(())
}

use std::sync::Arc;

use mivia_application::errors::app_error;
use mivia_application::settings::{GlobalSettings, Integration, global_settings, update_global};
use mivia_domain::naming::Language;
use mivia_os::{Desired, ShellIntegration, ShellPlaces};
use mivia_ports::ipc::error::AppError;
use mivia_ports::ipc::setup::{DoneProblem, IntegrationView, SetupLocks, StartupState};

use super::cx::{Cx, Services};
use super::setup::language;

const NAMES: [&str; 2] = ["Analyze with MiViA", "Mit MiViA auswerten"];

const fn entry_name(language: Language) -> &'static str {
    match language {
        Language::En => NAMES[0],
        Language::De => NAMES[1],
    }
}

fn shell() -> Option<ShellIntegration> {
    if cfg!(feature = "e2e") {
        return None;
    }
    let exe = std::env::current_exe().ok()?;
    ShellIntegration::for_install(ShellPlaces::user()?, &exe)
}

fn desired(integration: Integration, previous: Option<&str>, language: Language) -> Desired {
    let name = entry_name(language);
    let old = NAMES
        .into_iter()
        .chain(previous)
        .filter(|old| *old != name)
        .map(str::to_owned)
        .collect();
    Desired {
        autostart: integration.autostart,
        explorer: integration.explorer,
        verb_label: name.to_owned(),
        send_to_name: name.to_owned(),
        old_send_to_names: old,
    }
}

fn effective(settings: &GlobalSettings, services: &Services) -> Integration {
    Integration {
        autostart: services
            .policies
            .autostart
            .unwrap_or(settings.integration.autostart),
        explorer: services
            .policies
            .explorer
            .unwrap_or(settings.integration.explorer),
    }
}

async fn remember_name(services: &Services, language: Language) {
    let name = entry_name(language).to_owned();
    if let Err(error) = update_global(services.store.as_ref(), move |settings| {
        settings.send_to_name = Some(name);
    })
    .await
    {
        tracing::warn!(?error, "settings.writeFailed");
    }
}

pub(crate) fn apply_after_setup(
    services: &Arc<Services>,
    integration: Integration,
    language: Language,
) -> Vec<DoneProblem> {
    let Some(shell) = shell() else {
        tracing::info!("integration.notInstalled");
        return Vec::new();
    };
    let applied = shell.apply(&desired(integration, None, language));
    if let Err(error) = shell.mark_setup_completed(&services.version) {
        tracing::warn!(%error, "integration.markerFailed");
    }
    let services = Arc::clone(services);
    tauri::async_runtime::spawn(async move { remember_name(&services, language).await });
    let mut problems = Vec::new();
    if let Err(error) = applied.autostart {
        tracing::warn!(%error, "integration.autostartFailed");
        problems.push(DoneProblem::Autostart);
    }
    if let Err(error) = applied.explorer {
        tracing::warn!(%error, "integration.explorerFailed");
        problems.push(DoneProblem::Explorer);
    }
    problems
}

pub(crate) async fn heal(services: &Services, language: Language) {
    let Some(shell) = shell() else {
        return;
    };
    let Ok(settings) = global_settings(services.store.as_ref()).await else {
        return;
    };
    if !settings.setup_completed {
        let on = services.policies.autostart.unwrap_or(true);
        if let Err(error) = shell.autostart(on) {
            tracing::warn!(%error, "integration.autostartFailed");
        }
        return;
    }
    let wanted = desired(
        effective(&settings, services),
        settings.send_to_name.as_deref(),
        language,
    );
    let applied = shell.apply(&wanted);
    tracing::info!(
        changed = applied.changed,
        autostart = applied.autostart.is_ok(),
        explorer = applied.explorer.is_ok(),
        "integration.applied"
    );
    remember_name(services, language).await;
}

fn locks(services: &Services) -> SetupLocks {
    SetupLocks {
        language: services.policies.language.is_some(),
        legacy_search: services.policies.legacy_migration != Some(false),
        autostart: services.policies.autostart,
        explorer: services.policies.explorer,
        telemetry: services.policies.telemetry,
    }
}

pub(crate) async fn view(cx: &Cx) -> Result<IntegrationView, AppError> {
    let ready = cx.ready().await;
    let settings = global_settings(ready.store.as_ref())
        .await
        .map_err(|error| app_error(&error, "get_integration"))?;
    let shell = shell();
    let wanted = effective(&settings, &ready.services);
    let autostart = shell.as_ref().map_or(
        if wanted.autostart {
            StartupState::On
        } else {
            StartupState::Off
        },
        ShellIntegration::startup_state,
    );
    Ok(IntegrationView {
        available: shell.is_some(),
        autostart,
        explorer: wanted.explorer,
        locks: locks(&ready.services),
    })
}

pub(crate) async fn set(
    cx: &Cx,
    autostart: Option<bool>,
    explorer: Option<bool>,
) -> Result<IntegrationView, AppError> {
    let ready = cx.ready().await;
    update_global(ready.store.as_ref(), move |settings| {
        settings.integration.autostart = autostart.unwrap_or(settings.integration.autostart);
        settings.integration.explorer = explorer.unwrap_or(settings.integration.explorer);
    })
    .await
    .map_err(|error| app_error(&error, "set_integration"))?;
    heal(&ready.services, language(&ready).await).await;
    view(cx).await
}

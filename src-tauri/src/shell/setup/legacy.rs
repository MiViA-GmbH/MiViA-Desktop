use std::io::Read;
use std::path::Path;

use mivia_application::legacy::{LegacyConfig, MAX_CONFIG_BYTES, blank_access_token, parse};
use mivia_application::settings::{PendingStop, update_global};
use mivia_application::setup::{SetupDraft, read_draft};
use mivia_os::{
    LegacyInstall, StartupEntry, is_network, legacy_at, legacy_processes, legacy_startup,
    remove_user_startup,
};
use mivia_ports::ipc::error::AppError;
use mivia_ports::ipc::setup::{
    LegacyFlag, LegacyOutcome, LegacyTake, LegacyView, SetupOption, SetupView,
};
use mivia_ports::store::StoreExt;

use super::super::cx::{Cx, Services};
use super::wizard::{failed, view};

const LEGACY_ATTEMPTS: u32 = 5;

fn stop_legacy(install: &LegacyInstall) -> LegacyOutcome {
    if !legacy_processes(&install.exe).is_empty() {
        return LegacyOutcome::Close;
    }
    let blanked = std::fs::read(&install.config)
        .ok()
        .and_then(|bytes| blank_access_token(&bytes).ok())
        .is_some_and(|bytes| std::fs::write(&install.config, bytes).is_ok());
    let removed = legacy_startup(&install.exe)
        .iter()
        .all(|entry| remove_user_startup(entry).is_ok());
    if blanked && removed {
        LegacyOutcome::Stopped
    } else {
        LegacyOutcome::Failed
    }
}

fn machine_entries(install: &LegacyInstall) -> Vec<String> {
    legacy_startup(&install.exe)
        .into_iter()
        .filter_map(|entry| match entry {
            StartupEntry::MachineLink(path) => Some(path.display().to_string()),
            StartupEntry::MachineRun(name) => Some(name),
            StartupEntry::UserLink(_) | StartupEntry::UserRun(_) => None,
        })
        .collect()
}

pub(crate) async fn retry_legacy_stop(services: &Services) {
    let Ok(settings) = mivia_application::settings::global_settings(services.store.as_ref()).await
    else {
        return;
    };
    let Some(pending) = settings.legacy_stop else {
        return;
    };
    let outcome = legacy_at(Path::new(&pending.folder)).map(|install| stop_legacy(&install));
    let done = outcome != Some(LegacyOutcome::Close) || pending.attempts + 1 >= LEGACY_ATTEMPTS;
    tracing::info!(
        ?outcome,
        attempts = pending.attempts + 1,
        "legacy.stopRetried"
    );
    let next = (!done).then(|| PendingStop {
        attempts: pending.attempts + 1,
        ..pending
    });
    if let Err(error) = update_global(services.store.as_ref(), move |settings| {
        settings.legacy_stop = next;
    })
    .await
    {
        tracing::warn!(?error, "settings.writeFailed");
    }
}

pub(super) async fn switch_off(
    cx: &Cx,
    draft: &SetupDraft,
    services: &Services,
) -> (Option<LegacyOutcome>, Vec<String>) {
    let Some(install) = legacy_install(cx, draft).filter(|install| !is_network(&install.folder))
    else {
        return (None, Vec::new());
    };
    if !draft.has(SetupOption::LegacyStop) || services.policies.legacy_migration == Some(false) {
        return (None, Vec::new());
    }
    let outcome = stop_legacy(&install);
    if outcome == LegacyOutcome::Close {
        let pending = PendingStop {
            folder: install.folder.display().to_string(),
            attempts: 0,
        };
        if let Err(error) = update_global(services.store.as_ref(), move |settings| {
            settings.legacy_stop = Some(pending);
        })
        .await
        {
            tracing::warn!(?error, "settings.writeFailed");
        }
    }
    (Some(outcome), machine_entries(&install))
}

pub(crate) fn read_config(install: &LegacyInstall) -> Option<LegacyConfig> {
    let limit = u64::try_from(MAX_CONFIG_BYTES).unwrap_or(u64::MAX) + 1;
    let mut bytes = Vec::new();
    std::fs::File::open(&install.config)
        .and_then(|file| file.take(limit).read_to_end(&mut bytes))
        .ok()?;
    parse(&bytes).ok()
}

pub(crate) fn legacy_install(cx: &Cx, draft: &SetupDraft) -> Option<LegacyInstall> {
    let chosen = draft
        .legacy_folder
        .as_deref()
        .map(Path::new)
        .and_then(legacy_at);
    chosen.or_else(|| cx.setup.legacy())
}

pub(super) fn legacy_view(install: &LegacyInstall, take: Option<LegacyTake>) -> LegacyView {
    let config = read_config(install);
    let mut flags = Vec::new();
    match &config {
        Some(config) if config.access_token.is_some() => flags.push(LegacyFlag::HasKey),
        Some(_) => {}
        None => flags.push(LegacyFlag::Unreadable),
    }
    if !legacy_processes(&install.exe).is_empty() {
        flags.push(LegacyFlag::Running);
    }
    if is_network(&install.folder) {
        flags.push(LegacyFlag::Network);
    }
    LegacyView {
        folder: install.folder.display().to_string(),
        input_folder: config
            .as_ref()
            .and_then(|config| config.input_directory.clone()),
        model_count: config.map_or(0, |config| {
            u32::try_from(config.models.len()).unwrap_or(u32::MAX)
        }),
        flags,
        take,
    }
}

pub(crate) async fn close_legacy(cx: &Cx) -> Result<SetupView, AppError> {
    let ready = cx.ready().await;
    let draft = ready
        .store
        .write(read_draft)
        .await
        .map_err(|error| failed(&error))?;
    if let Some(install) = legacy_install(cx, &draft) {
        let closed =
            tokio::task::spawn_blocking(move || mivia_os::close_legacy(&install.exe)).await;
        if !matches!(closed, Ok(Ok(()))) {
            tracing::warn!("legacy.closeFailed");
        }
    }
    view(cx).await
}

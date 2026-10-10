use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use mivia_application::errors::app_error;
use mivia_application::settings::{global_settings, update_global};
use mivia_ports::ipc::error::{AppError, AppErrorKind};
use mivia_ports::ipc::history::{BackupView, BackupsView};
use mivia_ports::store::StoreExt;
use mivia_ports::store::history::{NewEvent, Trigger};
use mivia_ports::time::{iso_utc, utc_date};
use mivia_store::{backup_series, daily_backup_name, prune_daily};
use tauri::{AppHandle, Manager};

use super::account;
use super::cx::{Cx, Ready, now};
use super::history::invalid;

const CHECK_EVERY: Duration = Duration::from_secs(3600);
const FIRST_CHECK: Duration = Duration::from_secs(60);
const PREFIX: &str = "mivia-";
const SUFFIX: &str = ".db";
const APP: &str = "app";
const FOLDER: &str = "folder";

fn app_dir(ready: &Ready) -> PathBuf {
    ready.services.data_dir.join("backups")
}

async fn folder(ready: &Ready) -> Option<PathBuf> {
    let settings = global_settings(ready.store.as_ref()).await.ok()?;
    settings.backup_folder.map(PathBuf::from)
}

fn copy_into(source: &Path, folder: &Path) -> std::io::Result<()> {
    let name = source.file_name().unwrap_or_default();
    let target = folder.join(name);
    if target.exists() {
        return Ok(());
    }
    let temp = folder.join("~mivia-backup.tmp");
    std::fs::copy(source, &temp)?;
    std::fs::rename(&temp, &target)?;
    prune_daily(folder)
}

async fn daily(ready: &Ready) {
    let dir = app_dir(ready);
    let at = now();
    let today = dir.join(daily_backup_name(&utc_date(at)));
    if !today.exists() {
        let made = std::fs::create_dir_all(&dir).map_err(|error| error.to_string());
        let made = match made {
            Ok(()) => ready
                .store
                .daily_backup(dir.clone(), at)
                .await
                .map_err(|error| format!("{error:?}")),
            Err(error) => Err(error),
        };
        if let Err(error) = made {
            tracing::warn!(%error, "backup.failed");
            return;
        }
    }
    let Some(folder) = folder(ready).await else {
        return;
    };
    match copy_into(&today, &folder) {
        Ok(()) => {
            let copied = update_global(ready.store.as_ref(), move |settings| {
                settings.backup_copied_at = Some(at.0);
            });
            drop(copied.await);
        }
        Err(error) => tracing::warn!(%error, "backup.copyFailed"),
    }
}

pub(crate) async fn run(app: AppHandle) {
    tokio::time::sleep(FIRST_CHECK).await;
    loop {
        let cx = app.state::<Cx>();
        if cx.completed() {
            let ready = cx.ready().await;
            daily(ready.as_ref()).await;
        }
        tokio::time::sleep(CHECK_EVERY).await;
    }
}

fn listed(dir: &Path, location: &str) -> Vec<(BackupView, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let date = name.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?.to_owned();
            let path = entry.path();
            let view = BackupView {
                id: format!("{location}:{name}"),
                series: backup_series(&path)?,
                in_folder: location == FOLDER,
                date,
            };
            Some((view, path))
        })
        .collect()
}

async fn all(ready: &Ready) -> Vec<(BackupView, PathBuf)> {
    let mut found = listed(&app_dir(ready), APP);
    if let Some(folder) = folder(ready).await {
        found.extend(listed(&folder, FOLDER));
    }
    found.sort_by(|a, b| b.0.date.cmp(&a.0.date).then(a.0.id.cmp(&b.0.id)));
    found
}

pub(crate) async fn view(cx: &Cx) -> Result<BackupsView, AppError> {
    let ready = cx.ready().await;
    let settings = global_settings(ready.store.as_ref())
        .await
        .map_err(|error| app_error(&error, "list_backups"))?;
    Ok(BackupsView {
        folder: settings.backup_folder,
        last_copy: settings
            .backup_copied_at
            .map(|at| iso_utc(mivia_domain::ids::Timestamp(at))),
        backups: all(&ready)
            .await
            .into_iter()
            .map(|(view, _)| view)
            .collect(),
    })
}

pub(crate) async fn set_folder(cx: &Cx, pick: Option<String>) -> Result<BackupsView, AppError> {
    let ready = cx.ready().await;
    let folder = match &pick {
        Some(id) => Some(
            cx.setup
                .pick(id)
                .ok_or_else(|| invalid("pickExpired", "set_backup_folder"))?,
        ),
        None => None,
    };
    let chosen = folder.map(|path| path.to_string());
    update_global(ready.store.as_ref(), move |settings| {
        settings.backup_folder = chosen;
    })
    .await
    .map_err(|error| app_error(&error, "set_backup_folder"))?;
    if let Some(id) = &pick {
        cx.setup.used_pick(id);
    }
    view(cx).await
}

pub(crate) async fn restore(cx: &Cx, id: &str) -> Result<(), AppError> {
    let ready = cx.ready().await;
    let path = all(&ready)
        .await
        .into_iter()
        .find(|(view, _)| view.id == id)
        .map(|(_, path)| path)
        .ok_or_else(|| invalid("backupNotFound", "restore_backup"))?;
    let at = now();
    cx.set(Ready::without_account(Arc::clone(&ready.services)));
    let restored = ready.store.restore_from(path, at).await;
    cx.set(account::compose(&ready.services, false, None).await);
    restored.map_err(|error| {
        let message = format!("{error:?}");
        AppError::new(
            AppErrorKind::Validation,
            Some("backupInvalid"),
            message,
            "restore_backup",
        )
    })?;
    let rebuilt = cx.ready().await;
    let scope = rebuilt.scope;
    let mut event = NewEvent::new("backup_restored", Trigger::User, at);
    event.after = Some(format!("{{\"backup\":{id:?}}}"));
    if let Err(error) = rebuilt
        .store
        .write(move |tx| tx.history(scope).log(&event))
        .await
    {
        tracing::warn!(?error, "backup.restoreNotLogged");
    }
    Ok(())
}

use std::path::Path;

use mivia_application::errors::app_error;
use mivia_application::settings::global_settings;
use mivia_application::setup::{DraftConnection, SetupDraft, read_draft, write_draft};
use mivia_domain::naming::Language;
use mivia_domain::path::WinPath;
use mivia_os::legacy_at;
use mivia_ports::ipc::error::{AppError, AppErrorKind};
use mivia_ports::ipc::folders::PickPurpose;
use mivia_ports::ipc::setup::{
    ConnectOutcome, ConnectionNote, LegacyTake, SetupInput, SetupLocks, SetupOption, SetupView,
};
use mivia_ports::store::{StoreError, StoreExt};
use tauri::AppHandle;

use super::super::account::{self, clear_clipboard_if, identify, key_from, store_key};
use super::super::cx::{Cx, Ready};
use super::super::folders::{self, folder_view, report_folder, report_view};
use super::language;
use super::legacy::{legacy_install, legacy_view, read_config};

const SITE: &str = "setup";

const DEFAULT_CONTACT: &str = "support@mivia.ai";

pub(super) fn failed(error: &StoreError) -> AppError {
    app_error(error, SITE)
}

fn invalid(code: &'static str) -> AppError {
    AppError::new(AppErrorKind::Validation, Some(code), code.to_owned(), SITE)
}

fn prefill(cx: &Cx, ready: &Ready, draft: &SetupDraft) -> Option<String> {
    let legacy = (draft.legacy_take == Some(LegacyTake::All))
        .then(|| legacy_install(cx, draft).and_then(|install| read_config(&install)))
        .flatten()
        .and_then(|config| config.input_directory);
    legacy.or_else(|| ready.services.policies.default_input_folder.clone())
}

fn locks(ready: &Ready) -> SetupLocks {
    let policies = &ready.services.policies;
    SetupLocks {
        language: policies.language.is_some(),
        legacy_search: policies.legacy_migration != Some(false),
        autostart: policies.autostart,
        explorer: policies.explorer,
        telemetry: policies.telemetry,
    }
}

fn detected_language() -> String {
    let german = mivia_os::system_locale()
        .display_language
        .is_some_and(|tag| tag.starts_with("de"));
    if german { "Deutsch" } else { "English" }.to_owned()
}

async fn update<R, F>(ready: &Ready, change: F) -> Result<R, AppError>
where
    R: Send + 'static,
    F: FnOnce(&mut SetupDraft) -> R + Send + 'static,
{
    ready
        .store
        .write(move |tx| {
            let mut draft = read_draft(tx)?;
            let result = change(&mut draft);
            write_draft(tx, &draft)?;
            Ok(result)
        })
        .await
        .map_err(|error| failed(&error))
}

async fn folder_parts(ready: &Ready, draft: &mut SetupDraft, language: Language) -> SetupView {
    let mut view = base_view(ready, draft);
    let Some(folder) = draft.folder_path() else {
        return view;
    };
    view.folder = Some(folder_view(ready, &folder, language).await);
    let custom = draft
        .report_custom
        .as_deref()
        .and_then(|text| WinPath::parse(text).ok());
    let chosen = report_folder(draft.report_mode, custom.as_ref(), language);
    let (report, effective) = report_view(&folder, &chosen, language).await;
    if report.denied.is_some() {
        draft.report_mode = folders::mode_of(&effective);
        draft.report_custom = Some(report.display.clone());
    }
    view.report = Some(report);
    view
}

fn base_view(ready: &Ready, draft: &SetupDraft) -> SetupView {
    let services = &ready.services;
    SetupView {
        completed: false,
        step: draft.step,
        resumed: false,
        host: services
            .origin
            .0
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .to_owned(),
        support_contact: services
            .policies
            .support_contact
            .clone()
            .unwrap_or_else(|| DEFAULT_CONTACT.to_owned()),
        detected_language: detected_language(),
        locks: locks(ready),
        legacy: None,
        legacy_folder_missing: None,
        connection: draft.connection.as_ref().map(DraftConnection::view),
        folder: None,
        preset: draft.preset,
        report: None,
        models: draft.model_views(),
        options: draft.options.iter().copied().collect(),
    }
}

pub(crate) async fn view(cx: &Cx) -> Result<SetupView, AppError> {
    let ready = cx.ready().await;
    let completed = global_settings(ready.store.as_ref())
        .await
        .map_err(|error| failed(&error))?
        .setup_completed;
    let mut draft = ready
        .store
        .write(read_draft)
        .await
        .map_err(|error| failed(&error))?;
    let language = language(&ready).await;
    let candidate = (draft.folder.is_none() && !draft.has(SetupOption::SkipFolder))
        .then(|| prefill(cx, &ready, &draft))
        .flatten();
    let missing = candidate.clone().filter(|path| !Path::new(path).is_dir());
    if let Some(path) = candidate.filter(|path| Path::new(path).is_dir()) {
        draft.folder = Some(path);
    }
    let before = draft.clone();
    let mut view = folder_parts(&ready, &mut draft, language).await;
    if draft != before || view.folder.is_some() {
        let saved = draft.clone();
        ready
            .store
            .write(move |tx| write_draft(tx, &saved))
            .await
            .map_err(|error| failed(&error))?;
    }
    view.completed = completed;
    view.resumed = cx.setup.resumed();
    view.legacy_folder_missing = missing.filter(|_| draft.legacy_take == Some(LegacyTake::All));
    let search = view.locks.legacy_search;
    view.legacy = legacy_install(cx, &draft)
        .filter(|_| search)
        .map(|install| legacy_view(&install, draft.legacy_take));
    Ok(view)
}

pub(crate) async fn save(cx: &Cx, input: SetupInput) -> Result<SetupView, AppError> {
    let ready = cx.ready().await;
    let found = cx
        .setup
        .legacy()
        .map(|install| install.folder.display().to_string());
    update(&ready, move |draft| {
        if input.legacy_take.is_some() && draft.legacy_folder.is_none() {
            draft.legacy_folder = found;
        }
        draft.apply(input);
    })
    .await?;
    view(cx).await
}

pub(crate) async fn pick(
    cx: &Cx,
    app: &AppHandle,
    purpose: PickPurpose,
) -> Result<SetupView, AppError> {
    let ready = cx.ready().await;
    let Some(path) = folders::pick(app, "MiViA Desktop").await else {
        return view(cx).await;
    };
    let text = path.to_string();
    match purpose {
        PickPurpose::Input => {
            update(&ready, move |draft| {
                draft.folder = Some(text);
                draft.options.remove(&SetupOption::SkipFolder);
            })
            .await?;
        }
        PickPurpose::Reports => {
            update(&ready, move |draft| {
                draft.report_mode = mivia_ports::ipc::setup::ReportMode::Custom;
                draft.report_custom = Some(text);
            })
            .await?;
        }
        PickPurpose::Legacy => {
            let install =
                legacy_at(Path::new(&text)).ok_or_else(|| invalid("legacyNotInFolder"))?;
            cx.setup.found_legacy(Some(install));
            update(&ready, move |draft| draft.legacy_folder = Some(text)).await?;
        }
    }
    view(cx).await
}

async fn connect_key(
    cx: &Cx,
    key: secrecy::SecretString,
    imported: bool,
) -> Result<ConnectOutcome, AppError> {
    let ready = cx.ready().await;
    let identity = match identify(&ready.services, &key).await {
        Ok(identity) => identity,
        Err(outcome @ ConnectOutcome::Offline { .. }) => {
            cx.setup.hold_pending(key);
            return Ok(outcome);
        }
        Err(outcome) => return Ok(outcome),
    };
    cx.setup.forget_keys();
    let mut stored = store_key(&ready.services, &identity, &key).await;
    if stored.notes.contains(&ConnectionNote::StoreFailed) {
        cx.setup.keep_session_key(key.clone());
    }
    if imported {
        stored.notes.push(ConnectionNote::Imported);
    }
    let connection = DraftConnection {
        origin: ready.services.origin.0.clone(),
        account_id: identity.account_id,
        name: identity.name,
        email: identity.email,
        notes: stored.notes,
        remaining_uploads: stored.remaining_uploads,
    };
    let view = connection.view();
    let other_account = update(&ready, move |draft| draft.connect(connection)).await?;
    Ok(ConnectOutcome::Connected {
        connection: view,
        other_account,
        clipboard_cleared: clear_clipboard_if(&key),
    })
}

pub(crate) async fn connect(cx: &Cx, key: Option<String>) -> Result<ConnectOutcome, AppError> {
    let key = match key {
        Some(raw) => key_from(&raw),
        None => cx.setup.pending(),
    };
    let Some(key) = key else {
        return Ok(ConnectOutcome::Malformed);
    };
    if cx.completed() {
        return account::replace_key(cx, key).await;
    }
    connect_key(cx, key, false).await
}

pub(crate) async fn import_legacy(cx: &Cx) -> Result<ConnectOutcome, AppError> {
    let ready = cx.ready().await;
    let draft = ready
        .store
        .write(read_draft)
        .await
        .map_err(|error| failed(&error))?;
    let token = legacy_install(cx, &draft)
        .and_then(|install| read_config(&install))
        .and_then(|config| config.access_token)
        .and_then(|token| key_from(&token));
    match token {
        Some(key) => connect_key(cx, key, true).await,
        None => Ok(ConnectOutcome::Rejected),
    }
}

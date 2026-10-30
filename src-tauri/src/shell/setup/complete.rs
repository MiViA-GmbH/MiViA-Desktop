use std::sync::Arc;
use std::sync::atomic::Ordering;

use mivia_application::errors::{app_error, intake_error};
use mivia_application::intake::{AddFolder, PreparedFolder};
use mivia_application::report_settings::ReportSettings;
use mivia_application::settings::Integration;
use mivia_application::setup::{Completion, SetupDraft, complete as commit, quiet_ms, read_draft};
use mivia_domain::ids::FolderId;
use mivia_domain::label::LabelRule;
use mivia_domain::naming::Language;
use mivia_domain::path::WinPath;
use mivia_ports::auth::StoredCredential;
use mivia_ports::ipc::error::{AppError, AppErrorKind};
use mivia_ports::ipc::setup::{
    ConnectOutcome, ConnectionNote, DoneProblem, DoneView, LegacyOutcome, SetupOption,
    WaitingReason,
};
use mivia_ports::store::{AccountScope, NewAccount, StoreExt};
use secrecy::SecretString;
use tauri::AppHandle;

use super::super::account::{identify, rebuild};
use super::super::cx::{Cx, Ready, Services};
use super::super::folders::{display, folder_view, report_folder, report_view};
use super::super::{intake, integration, telemetry, tray};
use super::language;
use super::legacy::switch_off;

const SITE: &str = "complete_setup";

fn invalid(code: &str) -> AppError {
    AppError::new(AppErrorKind::Validation, Some(code), code.to_owned(), SITE)
}

async fn stored_key(cx: &Cx, services: &Services, account_id: &str) -> Option<SecretString> {
    let stored = services
        .credentials
        .load(&services.origin)
        .await
        .ok()
        .flatten();
    let found = stored.and_then(
        |StoredCredential::AccessKey {
             account_id: owner,
             key,
         }| { (owner == account_id).then_some(key) },
    );
    found.or_else(|| cx.setup.session_key())
}

async fn prepared(
    ready: &Ready,
    draft: &SetupDraft,
    language: Language,
) -> Result<Option<PreparedFolder>, AppError> {
    let Some(path) = draft
        .folder_path()
        .filter(|_| !draft.has(SetupOption::SkipFolder))
    else {
        return Ok(None);
    };
    let custom = draft
        .report_custom
        .as_deref()
        .and_then(|text| WinPath::parse(text).ok());
    let chosen = report_folder(draft.report_mode, custom.as_ref(), language);
    let (_, effective) = report_view(&path, &chosen, language).await;
    let input = AddFolder {
        path,
        model_choices: draft.choices(),
        quiet_ms: quiet_ms(draft.preset),
        label_rule: LabelRule::Auto,
        include_existing: draft.has(SetupOption::IncludeExisting),
        report_settings: ReportSettings::for_language(language),
        report_folder: effective,
    };
    let probe = intake::compose(
        Arc::clone(&ready.store),
        ready.scope,
        &ready.services.version,
    );
    probe
        .prepare_folder(input)
        .await
        .map(Some)
        .map_err(|error| intake_error(&error, SITE))
}

fn chosen(locked: Option<bool>, draft: &SetupDraft, option: SetupOption) -> bool {
    locked.unwrap_or_else(|| draft.has(option))
}

struct Checked {
    language: Language,
    account: NewAccount,
    waiting: Vec<WaitingReason>,
    folder: Option<PreparedFolder>,
}

async fn check(
    cx: &Cx,
    ready: &Ready,
    draft: &SetupDraft,
    language: Language,
) -> Result<Checked, AppError> {
    let services = &ready.services;
    let connection = draft
        .connection
        .clone()
        .ok_or_else(|| invalid("keyMissing"))?;
    let key = stored_key(cx, services, &connection.account_id)
        .await
        .ok_or_else(|| invalid("keyMissing"))?;
    let mut waiting = Vec::new();
    match identify(services, &key).await {
        Ok(_) => {}
        Err(ConnectOutcome::Offline { .. }) => waiting.push(WaitingReason::Offline),
        Err(_) => return Err(invalid("keyRejected")),
    }
    if connection.notes.contains(&ConnectionNote::LicenseInactive) {
        waiting.push(WaitingReason::License);
    }
    let folder = prepared(ready, draft, language).await?;
    if folder
        .as_ref()
        .is_some_and(|folder| folder.folder.model_choices.is_empty())
    {
        waiting.insert(0, WaitingReason::NoModels);
    }
    let account = NewAccount {
        server_url: services.origin.0.clone(),
        server_account_id: connection.account_id,
        display_name: connection.name,
        email: connection.email,
        issued_key_id: connection.issued_key_id,
    };
    Ok(Checked {
        language,
        account,
        waiting,
        folder,
    })
}

pub(crate) async fn complete(cx: &Cx, app: &AppHandle) -> Result<DoneView, AppError> {
    let ready = cx.ready().await;
    let services = Arc::clone(&ready.services);
    let draft = ready
        .store
        .write(read_draft)
        .await
        .map_err(|error| app_error(&error, SITE))?;
    let language = language(&ready).await;
    let checked = check(cx, &ready, &draft, language).await?;
    let completion = completion(&checked, &draft, &services);
    let integration = completion.integration;
    let (scope, folder) = ready
        .store
        .write(move |tx| commit(tx, &completion))
        .await
        .map_err(|error| app_error(&error, SITE))?;
    cx.setup_completed.store(true, Ordering::SeqCst);
    let begin = Begin {
        scope,
        folder,
        include: draft.has(SetupOption::IncludeExisting),
        integration,
        language,
    };
    let problems = start(cx, app, &services, begin).await;
    telemetry::setup_finished(&services, &draft, folder, integration).await;
    let (legacy, legacy_startup) = switch_off(cx, &draft, &services).await;
    let effects = Effects {
        problems,
        legacy,
        legacy_startup,
    };
    Ok(done_view(&ready, &draft, checked, effects).await)
}

fn completion(checked: &Checked, draft: &SetupDraft, services: &Services) -> Completion {
    let policies = &services.policies;
    Completion {
        account: checked.account.clone(),
        folder: checked.folder.clone(),
        telemetry: chosen(policies.telemetry, draft, SetupOption::Telemetry),
        integration: Integration {
            autostart: chosen(policies.autostart, draft, SetupOption::Autostart),
            explorer: chosen(policies.explorer, draft, SetupOption::Explorer),
        },
    }
}

struct Begin {
    scope: AccountScope,
    folder: Option<FolderId>,
    include: bool,
    integration: Integration,
    language: Language,
}

async fn start(
    cx: &Cx,
    app: &AppHandle,
    services: &Arc<Services>,
    begin: Begin,
) -> Vec<DoneProblem> {
    if let Some(id) = begin.folder {
        let intake = intake::compose_live(services, begin.scope).await;
        if let Err(error) = intake.start_folder(id, begin.include).await {
            tracing::warn!(?error, "setup.folderStartFailed");
        }
    }
    rebuild(cx, false).await;
    tray::show(app);
    integration::apply_after_setup(services, begin.integration, begin.language)
}

struct Effects {
    problems: Vec<DoneProblem>,
    legacy: Option<LegacyOutcome>,
    legacy_startup: Vec<String>,
}

async fn done_view(
    ready: &Ready,
    draft: &SetupDraft,
    checked: Checked,
    effects: Effects,
) -> DoneView {
    let folder = match checked.folder.as_ref() {
        Some(prepared) => Some(folder_view(ready, &prepared.folder.path, checked.language).await),
        None => None,
    };
    let report_folder = checked
        .folder
        .as_ref()
        .map(|prepared| display(&prepared.folder.path, &prepared.folder.report_folder));
    let existing = folder.as_ref().map_or(0, |view| view.existing);
    DoneView {
        name: checked.account.display_name,
        email: checked.account.email,
        folder_name: folder.as_ref().map(|view| view.name.clone()),
        folder: folder.map(|view| view.display),
        models: draft
            .choices()
            .into_iter()
            .map(|choice| choice.model_id)
            .collect(),
        report_folder,
        existing: if draft.has(SetupOption::IncludeExisting) {
            existing
        } else {
            0
        },
        waiting: checked.waiting,
        problems: effects.problems,
        explorer: draft.has(SetupOption::Explorer),
        legacy: effects.legacy,
        legacy_startup: effects.legacy_startup,
    }
}

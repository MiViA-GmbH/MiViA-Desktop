use std::io::Read;
use std::path::Path;

use mivia_application::errors::{app_error, not_found};
use mivia_application::one_off::{self, Confirmation, Context, OneOffError};
use mivia_application::report_settings::ReportSettings;
use mivia_application::spool::{self, MAX_SPOOL_BYTES, Spool};
use mivia_domain::image_format::ACCEPTED_EXTENSIONS;
use mivia_domain::naming::Language;
use mivia_domain::output::Format;
use mivia_domain::path::WinPath;
use mivia_os::{KnownFolder, OsFileSystem};
use mivia_ports::fs::FileSystem;
use mivia_ports::ipc::error::{AppError, AppErrorKind};
use mivia_ports::ipc::one_off::{OneOffInput, OneOffView};
use mivia_ports::ipc::screens;
use mivia_ports::server::ServerApi;
use mivia_ports::store::{AccountScope, StoreExt};
use tauri::{AppHandle, Manager};

use super::cx::{Cx, Ready, now};
use super::folders::{choices, known};
use super::{account, pipeline, setup, window};

const FALLBACK_FOLDER: &str = "MiViA reports";

fn scope(ready: &Ready) -> Option<AccountScope> {
    ready.intake.as_ref().map(|_| ready.scope)
}

pub(crate) async fn stage(app: &AppHandle, spool: Option<String>, paths: Vec<WinPath>) -> bool {
    let ready = app.state::<Cx>().ready().await;
    let scope = scope(&ready);
    let staged = ready
        .store
        .write(move |tx| one_off::stage(tx, scope, (spool.as_deref(), &paths), now()))
        .await;
    match staged {
        Ok(staged) => {
            reveal(app);
            staged
        }
        Err(error) => {
            tracing::error!(?error, "oneOff.stageFailed");
            false
        }
    }
}

fn reveal(app: &AppHandle) {
    let handle = app.clone();
    if let Err(error) = app.run_on_main_thread(move || window::reveal(&handle)) {
        tracing::warn!(%error, "window.openFailed");
    }
}

fn read_spool(path: &Path) -> Option<Spool> {
    let mut bytes = Vec::new();
    let limit = u64::try_from(MAX_SPOOL_BYTES).unwrap_or(u64::MAX) + 1;
    let read = std::fs::File::open(path).and_then(|file| file.take(limit).read_to_end(&mut bytes));
    match read.map(|_| spool::parse(&bytes)) {
        Ok(Ok(parsed)) => Some(parsed),
        Ok(Err(error)) => {
            tracing::warn!(?error, "instance.spoolRejected");
            None
        }
        Err(error) => {
            tracing::warn!(%error, "instance.spoolUnreadable");
            None
        }
    }
}

pub(crate) async fn ingest(app: &AppHandle, path: &Path) {
    let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
        return;
    };
    if let Some(parsed) = read_spool(path) {
        tracing::info!(
            spool_id = id,
            paths = parsed.paths.len(),
            rejected = parsed.rejected,
            non_unicode = parsed.non_unicode,
            "instance.handoff"
        );
        stage(app, Some(id.to_owned()), parsed.paths).await;
    }
    if let Err(error) = std::fs::remove_file(path) {
        tracing::warn!(%error, "instance.spoolNotRemoved");
    }
}

pub(crate) async fn ingest_inbox(app: &AppHandle, inbox: &Path) {
    let mut spools: Vec<_> = std::fs::read_dir(inbox)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    spools.sort();
    for path in spools {
        ingest(app, &path).await;
    }
}

struct Places {
    temp: Option<WinPath>,
    fallback: Option<WinPath>,
}

async fn places() -> Places {
    let temp = std::env::temp_dir()
        .to_str()
        .and_then(|text| WinPath::parse(text).ok());
    let resolved = match &temp {
        Some(temp) => OsFileSystem.final_path(temp).await.ok(),
        None => None,
    };
    Places {
        temp: resolved.or(temp),
        fallback: known(KnownFolder::Profile).and_then(|home| home.join(FALLBACK_FOLDER).ok()),
    }
}

fn context<'a>(ready: &'a Ready, scope: AccountScope, places: &'a Places) -> Context<'a> {
    Context {
        store: ready.store.as_ref(),
        fs: &OsFileSystem,
        scope,
        temp: places.temp.as_ref(),
        fallback: places.fallback.as_ref(),
    }
}

fn no_account(site: &str) -> AppError {
    not_found("noAccount", site)
}

pub(crate) async fn view(cx: &Cx) -> Result<Option<OneOffView>, AppError> {
    let ready = cx.ready().await;
    let Some(scope) = scope(&ready) else {
        return Ok(None);
    };
    let language = match setup::language(&ready).await {
        Language::De => screens::Language::De,
        Language::En => screens::Language::En,
    };
    let places = places().await;
    one_off::view(&context(&ready, scope, &places), language, now())
        .await
        .map_err(|error| app_error(&error, "get_one_off"))
}

fn settings(input: &OneOffInput) -> ReportSettings {
    let language = match input.language {
        screens::Language::De => Language::De,
        screens::Language::En => Language::En,
    };
    let mut settings = ReportSettings::for_language(language);
    settings.formats = [(input.pdf, Format::Pdf), (input.csv, Format::Csv)]
        .into_iter()
        .filter(|(on, _)| *on)
        .map(|(_, format)| format.code().to_owned())
        .collect();
    settings
}

fn confirm_error(error: &OneOffError) -> AppError {
    let code = match error {
        OneOffError::Store(error) => return app_error(error, "confirm_one_off"),
        OneOffError::NothingToAnalyze => "nothingToAnalyze",
        OneOffError::TooMany => "tooManyImages",
    };
    AppError::new(
        AppErrorKind::Validation,
        Some(code),
        code.to_owned(),
        "confirm_one_off",
    )
}

pub(crate) async fn confirm(cx: &Cx, input: OneOffInput) -> Result<String, AppError> {
    let ready = cx.ready().await;
    let scope = scope(&ready).ok_or_else(|| no_account("confirm_one_off"))?;
    let report_folder = match &input.report_pick {
        Some(pick) => Some(
            cx.setup
                .pick(pick)
                .ok_or_else(|| not_found("pickExpired", "confirm_one_off"))?,
        ),
        None => None,
    };
    let places = places().await;
    let confirmation = Confirmation {
        label: input.label.clone(),
        choices: choices(&input.models),
        settings: settings(&input),
        report_folder,
        app_version: ready.services.version.clone(),
    };
    let batch = one_off::confirm(&context(&ready, scope, &places), confirmation, now())
        .await
        .map_err(|error| confirm_error(&error))?;
    input
        .report_pick
        .iter()
        .for_each(|pick| cx.setup.used_pick(pick));
    ready.intake()?.wakers.readiness.notify_one();
    Ok(batch.0.to_string())
}

pub(crate) async fn discard(cx: &Cx, group: Option<u32>) -> Result<(), AppError> {
    let ready = cx.ready().await;
    let scope = scope(&ready).ok_or_else(|| no_account("discard_one_off"))?;
    let discarded = match group {
        None => {
            ready
                .store
                .write(move |tx| one_off::discard(tx, scope, now()))
                .await
        }
        Some(index) => {
            let places = places().await;
            let index = usize::try_from(index).unwrap_or(usize::MAX);
            one_off::remove_group(&context(&ready, scope, &places), index, now()).await
        }
    };
    discarded.map_err(|error| app_error(&error, "discard_one_off"))
}

fn filter() -> String {
    ACCEPTED_EXTENSIONS
        .iter()
        .map(|extension| format!("*.{extension}"))
        .collect::<Vec<_>>()
        .join(";")
}

pub(crate) async fn add_images(app: &AppHandle) -> Result<u32, AppError> {
    let owner = setup::owner(app);
    let picked =
        tokio::task::spawn_blocking(move || mivia_os::pick_files(owner, ("Images", &filter())))
            .await
            .map_err(|error| {
                AppError::new(AppErrorKind::Panic, None, error.to_string(), "add_images")
            })?
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "picker.failed");
                Vec::new()
            });
    let paths: Vec<WinPath> = picked
        .iter()
        .filter_map(|text| WinPath::parse(text).ok())
        .collect();
    let count = u32::try_from(paths.len()).unwrap_or(u32::MAX);
    if count > 0 {
        stage(app, None, paths).await;
    }
    Ok(count)
}

pub(crate) async fn remaining_uploads(cx: &Cx) -> Result<Option<u32>, AppError> {
    let ready = cx.ready().await;
    let Some(authorizer) = &ready.authorizer else {
        return Ok(None);
    };
    let license = pipeline::server_api(&ready.services, ready.scope.0, authorizer)
        .license()
        .await;
    Ok(license.ok().as_ref().and_then(account::remaining))
}

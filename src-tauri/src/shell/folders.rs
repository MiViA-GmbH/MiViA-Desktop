use std::path::Path;
use std::time::Duration;

use mivia_application::errors::intake_error;
use mivia_application::intake::AddFolder;
use mivia_application::report_settings::ReportSettings;
use mivia_application::setup::{KnownRoots, classify_folder, quiet_ms};
use mivia_domain::ids::ModelChoice;
use mivia_domain::label::LabelRule;
use mivia_domain::naming::Language;
use mivia_domain::output::{ProbeResult, ReportFolder};
use mivia_domain::path::WinPath;
use mivia_os::{KnownFolder, OsFileSystem, count_images, known_folder};
use mivia_ports::fs::FileSystem;
use mivia_ports::ipc::error::{AppError, AppErrorKind};
use mivia_ports::ipc::folders::{
    FolderBlock, FolderNote, FolderPick, FolderPickView, NewFolderInput, PickPurpose,
    ReportFolderView,
};
use mivia_ports::ipc::setup::{ModelChoiceView, ReportMode};
use mivia_ports::store::StoreExt;
use tauri::AppHandle;

use super::cx::{Cx, Ready};
use super::setup::language;

const COUNT_CAP: u32 = 10_000;
const COUNT_BUDGET: Duration = Duration::from_secs(2);
const LONG_PATH: usize = 150;
const ONE_DRIVE_VARS: [&str; 3] = ["OneDrive", "OneDriveCommercial", "OneDriveConsumer"];

fn win(path: &Path) -> Option<WinPath> {
    WinPath::parse(path.to_str()?).ok()
}

fn known(folder: KnownFolder) -> Option<WinPath> {
    known_folder(folder).as_deref().and_then(win)
}

fn roots(data_dir: &Path) -> KnownRoots {
    let windows = [
        KnownFolder::Windows,
        KnownFolder::ProgramFiles,
        KnownFolder::ProgramFilesX86,
    ]
    .into_iter()
    .filter_map(known)
    .chain(win(data_dir))
    .collect();
    let general = [
        KnownFolder::Desktop,
        KnownFolder::Documents,
        KnownFolder::Downloads,
        KnownFolder::Pictures,
    ]
    .into_iter()
    .filter_map(known)
    .collect();
    let one_drive = ONE_DRIVE_VARS
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .filter_map(|text| WinPath::parse(&text).ok())
        .collect();
    KnownRoots {
        windows,
        profile: known(KnownFolder::Profile),
        general,
        one_drive,
    }
}

pub(crate) fn name_of(path: &WinPath) -> String {
    path.file_name()
        .map_or_else(|| path.to_string(), str::to_owned)
}

pub(crate) async fn folder_view(
    ready: &Ready,
    path: &WinPath,
    language: Language,
) -> FolderPickView {
    let fs = OsFileSystem;
    let present = fs.stat(path).await.is_ok_and(|stat| stat.is_dir);
    let scope = ready.scope;
    let watched = ready
        .store
        .write(move |tx| tx.folders(scope).every_path())
        .await
        .unwrap_or_default();
    let (block, mut notes) = classify_folder(path, &roots(&ready.services.data_dir), &watched);
    let final_path = fs.final_path(path).await.ok();
    if let (Some(letter), Some(unc)) = (
        path.to_string().get(..2),
        final_path.filter(WinPath::is_unc),
    ) && !path.is_unc()
    {
        notes.retain(|note| *note != FolderNote::Network);
        notes.push(FolderNote::MappedDrive {
            letter: letter.to_owned(),
            unc: unc.to_string(),
        });
    }
    let text = path.to_string();
    let skip = language.report_folder_name();
    let countable = present && block.is_none();
    let counting = tokio::task::spawn_blocking(move || {
        countable.then(|| count_images(Path::new(&text), skip, COUNT_CAP, COUNT_BUDGET))
    });
    let counted = counting.await.ok().flatten();
    FolderPickView {
        display: path.to_string(),
        name: name_of(path),
        block: (!present).then_some(FolderBlock::NotFound).or(block),
        notes,
        existing: counted.as_ref().map_or(0, |count| count.count),
        existing_more: counted.is_some_and(|count| count.more),
    }
}

pub(crate) fn report_folder(
    mode: ReportMode,
    custom: Option<&WinPath>,
    language: Language,
) -> ReportFolder {
    match (mode, custom) {
        (ReportMode::Custom, Some(path)) => ReportFolder::Custom(path.clone()),
        (ReportMode::Beside, _) => ReportFolder::BesideSeries,
        (ReportMode::Subfolder | ReportMode::Custom, _) => {
            ReportFolder::Subfolder(language.report_folder_name().to_owned())
        }
    }
}

pub(crate) fn fallback(folder: &WinPath) -> Option<WinPath> {
    known(KnownFolder::Profile)?
        .join("MiViA reports")
        .ok()?
        .join(&name_of(folder))
        .ok()
}

pub(crate) async fn report_view(
    folder: &WinPath,
    chosen: &ReportFolder,
    language: Language,
) -> (ReportFolderView, ReportFolder) {
    let fs = OsFileSystem;
    let probe = fs
        .probe_folder(&chosen.base(folder), chosen.creates())
        .await
        .unwrap_or(ProbeResult::Denied);
    let (effective, denied) = match (probe, fallback(folder)) {
        (ProbeResult::Denied, Some(other)) => {
            (ReportFolder::Custom(other), Some(display(folder, chosen)))
        }
        _ => (chosen.clone(), None),
    };
    let shown = display(folder, &effective);
    let view = ReportFolderView {
        mode: mode_of(&effective),
        long_path: shown.encode_utf16().count() > LONG_PATH,
        display: shown,
        subfolder_name: language.report_folder_name().to_owned(),
        denied,
    };
    (view, effective)
}

pub(crate) const fn mode_of(folder: &ReportFolder) -> ReportMode {
    match folder {
        ReportFolder::Subfolder(_) => ReportMode::Subfolder,
        ReportFolder::BesideSeries => ReportMode::Beside,
        ReportFolder::Custom(_) => ReportMode::Custom,
    }
}

pub(crate) fn display(root: &WinPath, folder: &ReportFolder) -> String {
    match folder {
        ReportFolder::Subfolder(name) => root
            .join(name)
            .map_or_else(|_| root.to_string(), |path| path.to_string()),
        ReportFolder::BesideSeries => root.to_string(),
        ReportFolder::Custom(path) => path.to_string(),
    }
}

pub(crate) fn choices(models: &[ModelChoiceView]) -> Vec<ModelChoice> {
    models
        .iter()
        .map(|view| ModelChoice {
            model_id: view.model_id.clone(),
            customization_id: view.customization_id.clone(),
        })
        .collect()
}

pub(crate) async fn pick(app: &AppHandle, title: &str) -> Option<WinPath> {
    #[cfg(feature = "e2e")]
    if let Some(path) = super::e2e::picked_folder() {
        return WinPath::parse(&path).ok();
    }
    let owner = super::setup::owner(app);
    let title = title.to_owned();
    let picked = tokio::task::spawn_blocking(move || mivia_os::pick_folder(owner, &title)).await;
    match picked {
        Ok(Ok(Some(path))) => WinPath::parse(&path).ok(),
        Ok(Ok(None)) => None,
        Ok(Err(error)) => {
            tracing::warn!(%error, "picker.failed");
            None
        }
        Err(error) => {
            tracing::warn!(%error, "picker.panicked");
            None
        }
    }
}

pub(crate) async fn pick_new(
    cx: &Cx,
    app: &AppHandle,
    purpose: PickPurpose,
) -> Result<Option<FolderPick>, AppError> {
    let Some(path) = pick(app, "MiViA Desktop").await else {
        return Ok(None);
    };
    let ready = cx.ready().await;
    let language = language(&ready).await;
    let (folder, report) = match purpose {
        PickPurpose::Input => {
            let chosen = report_folder(ReportMode::Subfolder, None, language);
            let (report, _) = report_view(&path, &chosen, language).await;
            (folder_view(&ready, &path, language).await, Some(report))
        }
        PickPurpose::Reports | PickPurpose::Legacy => (plain_view(&path), None),
    };
    Ok(Some(FolderPick {
        pick_id: cx.setup.add_pick(path),
        folder,
        report,
    }))
}

fn plain_view(path: &WinPath) -> FolderPickView {
    FolderPickView {
        display: path.to_string(),
        name: name_of(path),
        block: None,
        notes: Vec::new(),
        existing: 0,
        existing_more: false,
    }
}

fn invalid(code: &'static str) -> AppError {
    AppError::new(
        AppErrorKind::Validation,
        Some(code),
        code.to_owned(),
        "add_folder",
    )
}

pub(crate) async fn add(cx: &Cx, input: NewFolderInput) -> Result<String, AppError> {
    let ready = cx.ready().await;
    let language = language(&ready).await;
    let path = cx
        .setup
        .pick(&input.folder_pick)
        .ok_or_else(|| invalid("pickExpired"))?;
    let custom = match &input.report_pick {
        Some(pick) => Some(cx.setup.pick(pick).ok_or_else(|| invalid("pickExpired"))?),
        None => None,
    };
    let chosen = report_folder(input.report_mode, custom.as_ref(), language);
    let (_, effective) = report_view(&path, &chosen, language).await;
    let folder = AddFolder {
        path,
        model_choices: choices(&input.models),
        quiet_ms: quiet_ms(input.preset),
        label_rule: LabelRule::Auto,
        include_existing: input.include_existing,
        report_settings: ReportSettings::for_language(language),
        report_folder: effective,
    };
    let id = ready
        .intake()?
        .add_folder(folder)
        .await
        .map_err(|error| intake_error(&error, "add_folder"))?;
    cx.setup.used_pick(&input.folder_pick);
    input
        .report_pick
        .iter()
        .for_each(|pick| cx.setup.used_pick(pick));
    Ok(id.0.to_string())
}

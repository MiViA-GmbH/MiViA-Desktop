mod expand;
mod pending;

use mivia_domain::batch::{ClosureReason, MAX_FILES};
use mivia_domain::ids::{BatchId, ModelChoice, Timestamp};
use mivia_domain::label::member_label;
use mivia_domain::naming;
use mivia_domain::path::WinPath;
use mivia_ports::fs::FileSystem;
use mivia_ports::ipc::one_off::OneOffView;
use mivia_ports::ipc::screens::Language;
use mivia_ports::ipc::setup::ModelChoiceView;
use mivia_ports::store::{
    AccountScope, BatchModelChoice, NewBatch, NewSourceFile, Origin, SentImages, SettingsScope,
    Store, StoreError, StoreExt, TxError, WriteTx,
};
use mivia_ports::time::iso_utc;

pub use pending::{discard, pending, stage};

use expand::{Expanded, ReportPlace, expand, report_target};
use pending::{in_group, keep_pending};

use crate::report_settings::ReportSettings;
use crate::settings::{GlobalSettings, LastChoices, write_global};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OneOffError {
    Store(StoreError),
    NothingToAnalyze,
    TooMany,
}

impl From<StoreError> for OneOffError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

const fn domain_language(language: Language) -> naming::Language {
    match language {
        Language::En => naming::Language::En,
        Language::De => naming::Language::De,
    }
}

fn last_choices(
    global: &GlobalSettings,
    scope: AccountScope,
) -> (Vec<ModelChoiceView>, Option<String>) {
    let Some(last) = global.one_off.get(&scope.0.0.to_string()) else {
        return (Vec::new(), None);
    };
    let views = last
        .models
        .iter()
        .filter_map(|key| ModelChoice::from_key(key))
        .map(|choice| ModelChoiceView {
            model_id: choice.model_id,
            customization_id: choice.customization_id,
        })
        .collect();
    (views, Some(iso_utc(Timestamp(last.at))))
}

pub struct Context<'a> {
    pub store: &'a dyn Store,
    pub fs: &'a dyn FileSystem,
    pub scope: AccountScope,
    pub temp: Option<&'a WinPath>,
    pub fallback: Option<&'a WinPath>,
}

struct Snapshot {
    sent: SentImages,
    global: GlobalSettings,
    folders: Vec<mivia_ports::store::intake::FolderRow>,
}

async fn snapshot(context: &Context<'_>, now: Timestamp) -> Result<Snapshot, StoreError> {
    let scope = context.scope;
    context
        .store
        .write(move |tx| {
            Ok(Snapshot {
                sent: pending(tx, scope, now)?,
                global: GlobalSettings::parse(tx.settings(SettingsScope::Global)?.as_deref()),
                folders: tx.folders(scope).list()?,
            })
        })
        .await
}

async fn place(
    context: &Context<'_>,
    snapshot: &Snapshot,
    first: &WinPath,
    language: Language,
) -> (WinPath, bool) {
    let place = ReportPlace {
        folders: &snapshot.folders,
        language: domain_language(language),
        fallback: context.fallback,
    };
    report_target(context.fs, first, &place).await
}

pub async fn view(
    context: &Context<'_>,
    language: Language,
    now: Timestamp,
) -> Result<Option<OneOffView>, StoreError> {
    let snapshot = snapshot(context, now).await?;
    if snapshot.sent.paths.is_empty() {
        return Ok(None);
    }
    let expanded = expand(context.fs, &snapshot.sent.paths, context.temp).await;
    let (report_folder, report_fallback) = match expanded.first() {
        Some(first) => {
            let (folder, fallback) = place(context, &snapshot, &first.path, language).await;
            (folder.to_string(), fallback)
        }
        None => (String::new(), false),
    };
    let (last_choices, last_chosen_at) = last_choices(&snapshot.global, context.scope);
    Ok(Some(OneOffView {
        count: u32::try_from(expanded.images.len()).unwrap_or(u32::MAX),
        groups: expanded.groups(),
        not_included: expanded.not_included.clone(),
        subfolders: expanded.subfolders,
        too_many: expanded.images.len() > MAX_FILES as usize,
        label: expanded.label(),
        last_choices,
        last_chosen_at,
        report_folder,
        report_fallback,
        language,
    }))
}

pub struct Confirmation {
    pub label: String,
    pub choices: Vec<ModelChoice>,
    pub settings: ReportSettings,
    pub report_folder: Option<WinPath>,
    pub app_version: String,
}

struct Plan {
    confirmation: Confirmation,
    expanded: Expanded,
    staged: Vec<String>,
    target: WinPath,
}

fn record_batch(
    tx: &mut dyn WriteTx,
    scope: AccountScope,
    plan: &Plan,
    now: Timestamp,
) -> Result<BatchId, TxError> {
    let mut settings = plan.confirmation.settings.clone();
    settings.one_off_folder = Some(plan.target.to_string());
    let label = plan.confirmation.label.trim();
    let batch = tx.batches(scope).open(&NewBatch {
        origin: Origin::OneOff,
        batch_key: format!("one-off:{}", now.0),
        label: if label.is_empty() {
            plan.expanded.label()
        } else {
            label.to_owned()
        },
        opened_at: now,
        report_settings_json: settings.json(),
        app_version: plan.confirmation.app_version.clone(),
        model_choices: plan
            .confirmation
            .choices
            .iter()
            .map(|choice| BatchModelChoice {
                choice: choice.clone(),
                names_json: "{}".to_owned(),
                model_version: None,
            })
            .collect(),
    })?;
    for image in &plan.expanded.images {
        tx.source_files(scope).add(&NewSourceFile {
            batch_id: batch,
            path: image.path.clone(),
            label: member_label(&[image.path.file_name().unwrap_or_default().to_owned()]),
            size: image.size,
            mtime: image.mtime,
            next_attempt_at: now,
        })?;
    }
    tx.batches(scope).close(batch, ClosureReason::OneOff, now)?;
    Ok(batch)
}

fn commit(
    tx: &mut dyn WriteTx,
    scope: AccountScope,
    plan: &Plan,
    now: Timestamp,
) -> Result<BatchId, TxError> {
    let batch = record_batch(tx, scope, plan, now)?;
    let mut left = pending(tx, scope, now)?;
    left.paths.retain(|path| !plan.staged.contains(path));
    keep_pending(tx, scope, &left, now)?;
    let mut global = GlobalSettings::parse(tx.settings(SettingsScope::Global)?.as_deref());
    let models = plan
        .confirmation
        .choices
        .iter()
        .map(ModelChoice::key)
        .collect();
    global
        .one_off
        .insert(scope.0.0.to_string(), LastChoices { models, at: now.0 });
    write_global(tx, &global)?;
    Ok(batch)
}

pub async fn confirm(
    context: &Context<'_>,
    confirmation: Confirmation,
    now: Timestamp,
) -> Result<BatchId, OneOffError> {
    let snapshot = snapshot(context, now).await?;
    let expanded = expand(context.fs, &snapshot.sent.paths, context.temp).await;
    let Some(first) = expanded.first().map(|image| image.path.clone()) else {
        return Err(OneOffError::NothingToAnalyze);
    };
    if expanded.images.len() > MAX_FILES as usize {
        return Err(OneOffError::TooMany);
    }
    let language = match naming::Language::from_code(&confirmation.settings.language) {
        Some(naming::Language::De) => Language::De,
        _ => Language::En,
    };
    let target = match &confirmation.report_folder {
        Some(folder) => folder.clone(),
        None => place(context, &snapshot, &first, language).await.0,
    };
    let plan = Plan {
        confirmation,
        expanded,
        staged: snapshot.sent.paths,
        target,
    };
    let scope = context.scope;
    Ok(context
        .store
        .write(move |tx| commit(tx, scope, &plan, now))
        .await?)
}

pub async fn remove_group(
    context: &Context<'_>,
    index: usize,
    now: Timestamp,
) -> Result<(), StoreError> {
    let snapshot = snapshot(context, now).await?;
    let groups = expand(context.fs, &snapshot.sent.paths, context.temp)
        .await
        .groups();
    let Some(folder) = groups
        .get(index)
        .and_then(|group| WinPath::parse(&group.folder).ok())
    else {
        return Ok(());
    };
    let scope = context.scope;
    context
        .store
        .write(move |tx| {
            let mut left = pending(tx, scope, now)?;
            left.paths.retain(|text| !in_group(text, &folder));
            keep_pending(tx, scope, &left, now)
        })
        .await
}

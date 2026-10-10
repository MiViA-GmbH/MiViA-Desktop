use mivia_domain::batch::ClosureReason;
use mivia_domain::ids::{FolderId, ModelChoice, Timestamp};
use mivia_domain::label::LabelRule;
use mivia_domain::output::{ProbeResult, ReportFolder};
use mivia_domain::path::WinPath;
use mivia_ports::fs::FsError;
use mivia_ports::store::intake::{FolderChange, FolderRow};
use mivia_ports::store::{AccountScope, NewFolder, TxError, WriteTx};

use super::closer::{Closure, close_series};
use super::plan::Plan;
use super::scan::Mode;
use super::{Intake, IntakeError};
use crate::report_settings::ReportSettings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderProblem {
    NotFound,
    NotAFolder,
    Overlaps,
    ReportFolderDenied,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddFolder {
    pub path: WinPath,
    pub model_choices: Vec<ModelChoice>,
    pub quiet_ms: i64,
    pub label_rule: LabelRule,
    pub include_existing: bool,
    pub report_settings: ReportSettings,
    pub report_folder: ReportFolder,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedFolder {
    pub folder: NewFolder,
    pub probe: ProbeResult,
    pub include_existing: bool,
}

pub fn insert_folder(
    tx: &mut dyn WriteTx,
    scope: AccountScope,
    prepared: &PreparedFolder,
) -> Result<FolderId, TxError> {
    let position = u32::try_from(tx.folders(scope).list()?.len()).unwrap_or(u32::MAX);
    let id = tx.folders(scope).add(&NewFolder {
        position,
        ..prepared.folder.clone()
    })?;
    tx.folders(scope)
        .update(id, &FolderChange::Probe(prepared.probe))?;
    Ok(id)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resumed {
    pub waiting: usize,
    pub since: Timestamp,
}

const fn folder_error(error: FsError) -> IntakeError {
    IntakeError::Folder(match error {
        FsError::NotFound | FsError::NetworkUnavailable => FolderProblem::NotFound,
        FsError::AccessDenied
        | FsError::SharingViolation
        | FsError::DiskFull
        | FsError::NotADirectory
        | FsError::InvalidName
        | FsError::CloudOnly
        | FsError::Changed
        | FsError::Link
        | FsError::Exists
        | FsError::Io => FolderProblem::NotAFolder,
    })
}

impl Intake {
    async fn checked_root(
        &self,
        path: &WinPath,
        except: Option<FolderId>,
    ) -> Result<WinPath, IntakeError> {
        let fs = &self.ports.fs;
        let root = fs.final_path(path).await.map_err(folder_error)?;
        if !fs.stat(&root).await.map_err(folder_error)?.is_dir {
            return Err(IntakeError::Folder(FolderProblem::NotAFolder));
        }
        let own = match except {
            Some(id) => Some(self.folder(id).await?.path),
            None => None,
        };
        let others = self
            .write(|tx, scope| tx.folders(scope).every_path())
            .await?;
        let overlaps = others
            .iter()
            .filter(|other| own.as_ref() != Some(*other))
            .any(|other| other.overlaps(&root));
        if overlaps {
            return Err(IntakeError::Folder(FolderProblem::Overlaps));
        }
        Ok(root)
    }

    async fn probe(
        &self,
        root: &WinPath,
        folder: &ReportFolder,
    ) -> Result<ProbeResult, IntakeError> {
        let base = folder.base(root);
        let result = self
            .ports
            .fs
            .probe_folder(&base, folder.creates())
            .await
            .unwrap_or(ProbeResult::Denied);
        if result == ProbeResult::Denied {
            return Err(IntakeError::Folder(FolderProblem::ReportFolderDenied));
        }
        Ok(result)
    }

    pub async fn probe_report_folder(&self, id: FolderId) -> Result<ProbeResult, IntakeError> {
        let row = self.folder(id).await?;
        let base = row.report_folder.base(&row.path);
        let result = self
            .ports
            .fs
            .probe_folder(&base, row.report_folder.creates())
            .await
            .unwrap_or(ProbeResult::Denied);
        self.change(id, FolderChange::Probe(result)).await?;
        Ok(result)
    }

    async fn folder(&self, id: FolderId) -> Result<FolderRow, IntakeError> {
        self.write(move |tx, scope| tx.folders(scope).get(id)).await
    }

    async fn change(&self, id: FolderId, change: FolderChange) -> Result<FolderRow, IntakeError> {
        self.write(move |tx, scope| {
            tx.folders(scope).update(id, &change)?;
            tx.folders(scope).get(id)
        })
        .await
    }

    pub async fn prepare_folder(&self, input: AddFolder) -> Result<PreparedFolder, IntakeError> {
        let root = self.checked_root(&input.path, None).await?;
        let probe = self.probe(&root, &input.report_folder).await?;
        Ok(PreparedFolder {
            folder: NewFolder {
                path: root,
                position: 0,
                quiet_period_ms: input.quiet_ms,
                label_rule: input.label_rule,
                report_settings_json: input.report_settings.json(),
                report_folder: input.report_folder,
                model_choices: input.model_choices,
            },
            probe,
            include_existing: input.include_existing,
        })
    }

    pub async fn start_folder(
        &self,
        id: FolderId,
        include_existing: bool,
    ) -> Result<(), IntakeError> {
        let _serial = self.serial.lock().await;
        let row = self.folder(id).await?;
        self.scan(&row, Mode::Baseline).await?;
        if include_existing {
            self.detect_existing(&row, Timestamp(0)).await?;
        }
        Ok(())
    }

    pub async fn add_folder(&self, input: AddFolder) -> Result<FolderId, IntakeError> {
        let prepared = self.prepare_folder(input).await?;
        let include_existing = prepared.include_existing;
        let id = self
            .write(move |tx, scope| insert_folder(tx, scope, &prepared))
            .await?;
        self.start_folder(id, include_existing).await?;
        Ok(id)
    }

    pub async fn pause_folder(&self, id: FolderId) -> Result<(), IntakeError> {
        let _serial = self.serial.lock().await;
        self.change(id, FolderChange::Active(false)).await?;
        self.ports.watcher.unwatch(id);
        self.schedule.lock().due.remove(&id);
        Ok(())
    }

    pub async fn resume_folder(&self, id: FolderId) -> Result<Resumed, IntakeError> {
        let _serial = self.serial.lock().await;
        let since = self.now();
        let row = self.change(id, FolderChange::Active(true)).await?;
        let waiting = self.scan(&row, Mode::Baseline).await?;
        Ok(Resumed { waiting, since })
    }

    pub async fn analyze_existing(
        &self,
        id: FolderId,
        since: Timestamp,
    ) -> Result<(), IntakeError> {
        let _serial = self.serial.lock().await;
        let row = self.folder(id).await?;
        self.detect_existing(&row, since).await
    }

    async fn detect_existing(&self, row: &FolderRow, since: Timestamp) -> Result<(), IntakeError> {
        let folder = row.id;
        let paths = self
            .write(move |tx, scope| tx.intake(scope).take_baseline(folder, since))
            .await?;
        let mut plan = Plan::new(row, Vec::new());
        for path in &paths {
            plan.detect_catch_up(self, path);
        }
        self.apply(plan.actions).await
    }

    pub async fn relocate_folder(&self, id: FolderId, path: &WinPath) -> Result<(), IntakeError> {
        let _serial = self.serial.lock().await;
        let root = self.checked_root(path, Some(id)).await?;
        self.ports.watcher.unwatch(id);
        self.remove_marker(&self.folder(id).await?).await;
        let row = self.change(id, FolderChange::Relocated(root)).await?;
        self.scan(&row, Mode::Detect).await.map(drop)
    }

    pub async fn remove_folder(&self, id: FolderId) -> Result<(), IntakeError> {
        let _serial = self.serial.lock().await;
        self.ports.watcher.unwatch(id);
        self.schedule.lock().due.remove(&id);
        self.remove_marker(&self.folder(id).await?).await;
        let now = self.now();
        self.write(move |tx, scope| {
            let open = tx.intake(scope).open_batches()?;
            for batch in open.iter().filter(|batch| batch.folder == id) {
                close_series(
                    tx,
                    scope,
                    Closure::plain(batch.id, id, ClosureReason::Manual),
                    now,
                )?;
            }
            tx.folders(scope).update(id, &FolderChange::Removed(now))
        })
        .await?;
        self.wake_all();
        Ok(())
    }
}

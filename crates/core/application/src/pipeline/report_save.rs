use std::collections::BTreeMap;

use mivia_domain::ids::ReportPartId;
use mivia_domain::naming::{Language, NameParts, ReportName, report_name};
use mivia_domain::output::{Format, ProbeResult, next_clash};
use mivia_domain::path::WinPath;
use mivia_domain::report::{PartEvent, SaveError, SaveProblem};
use mivia_ports::fs::{FsError, SaveTarget};
use mivia_ports::store::reports::{Rerequest, SaveRow};

use super::{Pipeline, PipelineError, Tick};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SavedCheck {
    Intact,
    Changed,
    Missing,
}

struct Plan {
    folder: WinPath,
    base: WinPath,
    create: bool,
    direct: bool,
    name: ReportName,
}

const fn save_error(error: FsError) -> SaveError {
    match error {
        FsError::AccessDenied => SaveError::Retryable(SaveProblem::Denied),
        FsError::DiskFull => SaveError::Retryable(SaveProblem::DiskFull),
        FsError::SharingViolation => SaveError::Retryable(SaveProblem::Busy),
        FsError::Link => SaveError::Retryable(SaveProblem::Moved),
        FsError::InvalidName => SaveError::InvalidName,
        FsError::Exists => SaveError::TooManyClashes,
        FsError::NotFound
        | FsError::NetworkUnavailable
        | FsError::NotADirectory
        | FsError::CloudOnly
        | FsError::Changed
        | FsError::Io => SaveError::Retryable(SaveProblem::Unreachable),
    }
}

fn home(row: &SaveRow) -> Option<(WinPath, WinPath, bool, bool)> {
    let root = row.root.as_ref()?;
    let mode = row.report_folder.as_ref()?;
    let member = row.first_member.as_ref();
    let relative = member.and_then(|path| root.relative_components(path));
    let folder = mode.target(root, relative.unwrap_or_default()).ok()?;
    let direct = row.probe == Some(ProbeResult::CreateOnly);
    Some((folder, mode.base(root), mode.creates(), direct))
}

fn localized(names_json: &str, language: Language) -> String {
    let names: BTreeMap<String, String> = serde_json::from_str(names_json).unwrap_or_default();
    names
        .get(language.code())
        .or_else(|| names.get(Language::En.code()))
        .cloned()
        .unwrap_or_default()
}

impl Pipeline {
    pub async fn save_tick(&self) -> Result<Tick, PipelineError> {
        let now = self.now();
        let rows = self
            .write(move |tx, scope| tx.reports(scope).save_due(now))
            .await?;
        if rows.is_empty() {
            let next = self
                .write(|tx, scope| tx.reports(scope).next_save())
                .await?;
            return Ok(next.map_or(Tick::Idle, Tick::DueAt));
        }
        for row in rows {
            let Some(plan) = self.plan(&row, None) else {
                tracing::warn!(part = row.id.0, "report.noFolder");
                continue;
            };
            self.save_part(&row, &plan, true).await?;
        }
        Ok(Tick::Busy)
    }

    pub async fn save_elsewhere(
        &self,
        id: ReportPartId,
        folder: WinPath,
    ) -> Result<(), PipelineError> {
        let row = self
            .write(move |tx, scope| tx.reports(scope).save_row(id))
            .await?;
        if let Some(plan) = self.plan(&row, Some(folder)) {
            self.save_part(&row, &plan, false).await?;
        }
        Ok(())
    }

    pub async fn discard_report_part(&self, id: ReportPartId) -> Result<(), PipelineError> {
        let row = self
            .write(move |tx, scope| tx.reports(scope).save_row(id))
            .await?;
        self.part_event(id, PartEvent::Discard).await?;
        self.remove_quietly(&row.staging).await;
        Ok(())
    }

    pub async fn check_saved_report(&self, id: ReportPartId) -> Result<SavedCheck, PipelineError> {
        let saved = self
            .write(move |tx, scope| tx.reports(scope).saved_file(id))
            .await?;
        let Some(saved) = saved else {
            return Ok(SavedCheck::Missing);
        };
        let check = match self.ports.fs.digest(&saved.path).await {
            Ok(found) if found == (saved.size, saved.sha256) => SavedCheck::Intact,
            Ok(_) => SavedCheck::Changed,
            Err(_) => SavedCheck::Missing,
        };
        if check == SavedCheck::Changed {
            tracing::warn!(part = id.0, "report.changedAfterSave");
        }
        Ok(check)
    }

    pub async fn try_again_report_part(&self, id: ReportPartId) -> Result<(), PipelineError> {
        let now = self.now();
        self.write(move |tx, scope| tx.reports(scope).rerequest(id, Rerequest::TryAgain, now))
            .await?;
        self.wakers.reports.notify_one();
        self.wakers.saver.notify_one();
        Ok(())
    }

    fn plan(&self, row: &SaveRow, elsewhere: Option<WinPath>) -> Option<Plan> {
        let (folder, base, create, direct) = match elsewhere {
            Some(folder) => (folder.clone(), folder, false, false),
            None => home(row)?,
        };
        let analysis = localized(&row.analysis_json, row.language);
        let variant = row
            .variant_json
            .as_deref()
            .map(|json| localized(json, row.language));
        let parts = NameParts {
            stamp: self.ports.clock.local_stamp(row.closed_at),
            label: &row.label,
            analysis: &analysis,
            variant: variant.as_deref(),
            part: Some(row.part),
            update: None,
            data: row.format == Format::Csv,
            language: row.language,
        };
        let name = report_name(&parts, folder.to_string().encode_utf16().count());
        Some(Plan {
            folder,
            base,
            create,
            direct,
            name,
        })
    }

    async fn adopted(&self, row: &SaveRow, plan: &Plan) -> Option<WinPath> {
        let intended = row.intended.as_ref()?;
        match self.ports.fs.digest(intended).await {
            Ok(found) if found == (row.size, row.sha256) => Some(intended.clone()),
            Ok(_) if plan.direct => {
                self.remove_quietly(intended).await;
                None
            }
            Ok(_) | Err(_) => None,
        }
    }

    async fn save_part(
        &self,
        row: &SaveRow,
        plan: &Plan,
        adopt: bool,
    ) -> Result<(), PipelineError> {
        let found = if adopt {
            self.adopted(row, plan).await
        } else {
            None
        };
        let outcome = match found {
            Some(path) => Ok(path),
            None => self.write_part(row, plan).await?,
        };
        let (id, now) = (row.id, self.now());
        match outcome {
            Ok(path) => {
                self.write(move |tx, scope| tx.reports(scope).saved(id, &path, now))
                    .await?;
                self.remove_quietly(&row.staging).await;
                tracing::info!(part = id.0, "report.saved");
            }
            Err(error) => {
                let state = self
                    .write(move |tx, scope| tx.reports(scope).save_failed(id, error, now))
                    .await?;
                tracing::warn!(part = id.0, ?error, ?state, "report.notSaved");
            }
        }
        Ok(())
    }

    async fn write_part(
        &self,
        row: &SaveRow,
        plan: &Plan,
    ) -> Result<Result<WinPath, SaveError>, PipelineError> {
        let fs = &self.ports.fs;
        let names = match fs.list_names(&plan.folder).await {
            Ok(names) => names,
            Err(FsError::NotFound) => Vec::new(),
            Err(error) => return Ok(Err(save_error(error))),
        };
        let Ok(clash) = next_clash(&names, &plan.name) else {
            return Ok(Err(SaveError::TooManyClashes));
        };
        let Ok(intended) = plan.folder.join(&plan.name.file_name(clash)) else {
            return Ok(Err(SaveError::InvalidName));
        };
        let id = row.id;
        self.write(move |tx, scope| tx.reports(scope).intend(id, &intended))
            .await?;
        let target = SaveTarget {
            folder: plan.folder.clone(),
            base: plan.base.clone(),
            name: plan.name.clone(),
            clash,
            temp_prefix: format!("~mivia-{id}-"),
            direct: plan.direct,
            create: plan.create,
        };
        Ok(fs
            .save_report(&row.staging, &target)
            .await
            .map_err(save_error))
    }
}

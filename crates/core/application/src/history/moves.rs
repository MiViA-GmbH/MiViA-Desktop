use mivia_domain::ids::{ReportId, Timestamp};
use mivia_domain::naming::ReportName;
use mivia_domain::path::WinPath;
use mivia_ports::fs::{FileSystem, FsError, SaveTarget};
use mivia_ports::store::StoreExt;
use mivia_ports::store::history::{MovablePart, MoveSet, Moved};

use super::{History, HistoryError};

struct Step {
    part: MovablePart,
    from: WinPath,
    to: WinPath,
}

fn kept_name(path: &WinPath) -> Result<ReportName, FsError> {
    let name = path.file_name().ok_or(FsError::InvalidName)?;
    let (stem, extension) = name.rsplit_once('.').ok_or(FsError::InvalidName)?;
    let extension = match extension.to_ascii_lowercase().as_str() {
        "pdf" => "pdf",
        "zip" => "zip",
        _ => return Err(FsError::InvalidName),
    };
    Ok(ReportName {
        stem: stem.to_owned(),
        extension,
        long_path: false,
    })
}

fn target(folder: WinPath, base: WinPath, name: ReportName) -> SaveTarget {
    SaveTarget {
        folder,
        base,
        name,
        clash: None,
        temp_prefix: "~mivia-move-".to_owned(),
        direct: false,
        create: true,
    }
}

async fn unchanged(fs: &dyn FileSystem, part: &MovablePart, path: &WinPath) -> bool {
    fs.digest(path)
        .await
        .is_ok_and(|found| found == (part.size, part.sha256))
}

async fn undo(fs: &dyn FileSystem, steps: &[Step]) {
    for step in steps.iter().rev() {
        let back = step
            .from
            .parent()
            .zip(kept_name(&step.from).ok())
            .map(|(folder, name)| target(folder.clone(), folder, name));
        let Some(back) = back else {
            continue;
        };
        if let Err(error) = fs.move_report(&step.to, &back).await {
            tracing::error!(part = step.part.id.0, ?error, "report.moveBackFailed");
        }
    }
}

async fn move_one(
    fs: &dyn FileSystem,
    part: &MovablePart,
    folder: WinPath,
    base: WinPath,
) -> Result<Step, HistoryError> {
    let name = kept_name(&part.path)?;
    let to = fs
        .move_report(&part.path, &target(folder, base, name))
        .await?;
    let step = Step {
        part: part.clone(),
        from: part.path.clone(),
        to,
    };
    if unchanged(fs, part, &step.to).await {
        return Ok(step);
    }
    undo(fs, std::slice::from_ref(&step)).await;
    Err(HistoryError::ChangedAfterMove)
}

async fn move_all(
    fs: &dyn FileSystem,
    set: &MoveSet,
    destination: impl Fn(&WinPath) -> Option<(WinPath, WinPath)>,
) -> Result<Vec<Step>, HistoryError> {
    let mut steps = Vec::new();
    for part in &set.parts {
        let Some((folder, base)) = destination(&part.path) else {
            undo(fs, &steps).await;
            return Err(HistoryError::Fs(FsError::InvalidName));
        };
        match move_one(fs, part, folder, base).await {
            Ok(step) => steps.push(step),
            Err(error) => {
                undo(fs, &steps).await;
                return Err(error);
            }
        }
    }
    Ok(steps)
}

async fn missing(fs: &dyn FileSystem, set: &MoveSet) -> Vec<String> {
    let mut gone = Vec::new();
    for part in &set.parts {
        if !unchanged(fs, part, &part.path).await {
            gone.push(part.path.file_name().unwrap_or_default().to_owned());
        }
    }
    gone
}

impl History {
    async fn load(&self, report: ReportId) -> Result<MoveSet, HistoryError> {
        let scope = self.scope;
        Ok(self
            .store
            .write(move |tx| tx.history(scope).move_set(report))
            .await?)
    }

    async fn record(&self, steps: Vec<Step>, how: Moved) -> Result<(), HistoryError> {
        let scope = self.scope;
        let moves: Vec<_> = steps
            .iter()
            .map(|step| (step.part.id, step.to.clone()))
            .collect();
        let written = self
            .store
            .write(move |tx| {
                moves
                    .iter()
                    .try_for_each(|(part, path)| tx.history(scope).moved(*part, path, how))
            })
            .await;
        if let Err(error) = written {
            undo(self.fs.as_ref(), &steps).await;
            return Err(error.into());
        }
        Ok(())
    }

    pub async fn move_to_replaced(
        &self,
        report: ReportId,
        now: Timestamp,
    ) -> Result<(), HistoryError> {
        let fs = self.fs.as_ref();
        let set = self.load(report).await?;
        let ready = set.replacement_saved && !set.moved;
        if !ready || set.unsaved > 0 || set.parts.is_empty() {
            return Err(HistoryError::NotReplaceable(Vec::new()));
        }
        let gone = missing(fs, &set).await;
        if !gone.is_empty() {
            return Err(HistoryError::NotReplaceable(gone));
        }
        let replaced = set.language.replaced_folder_name();
        let steps = move_all(fs, &set, |path| {
            let folder = path.parent()?;
            Some((folder.join(replaced).ok()?, folder))
        })
        .await?;
        self.record(steps, Moved::ToReplaced(now)).await
    }

    pub async fn restore_from_replaced(
        &self,
        report: ReportId,
        now: Timestamp,
    ) -> Result<(), HistoryError> {
        let fs = self.fs.as_ref();
        let set = self.load(report).await?;
        if !set.moved {
            return Err(HistoryError::NotMoved);
        }
        let gone = missing(fs, &set).await;
        if !gone.is_empty() {
            return Err(HistoryError::NotReplaceable(gone));
        }
        let steps = move_all(fs, &set, |path| {
            let original = path.parent()?.parent()?;
            Some((original.clone(), original))
        })
        .await?;
        self.record(steps, Moved::Restored(now)).await
    }
}

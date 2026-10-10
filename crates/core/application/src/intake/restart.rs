use mivia_domain::batch::Closing;
use mivia_domain::ids::Timestamp;
use mivia_domain::intake::{batch_key, is_catch_up};
use mivia_domain::path::WinPath;
use mivia_ports::store::intake::{FolderRow, OpenBatchRow};

use super::closer::{Closure, close_series, open_batch};
use super::plan::Plan;
use super::{Intake, IntakeError};

struct Found {
    row: FolderRow,
    files: Vec<(WinPath, Timestamp)>,
}

fn take_key(found: &mut Found, key: &str) -> Vec<(WinPath, Timestamp)> {
    let root = found.row.path.clone();
    let folder = found.row.id;
    let (mine, rest): (Vec<_>, Vec<_>) =
        std::mem::take(&mut found.files)
            .into_iter()
            .partition(|(path, _)| {
                root.relative_components(path)
                    .is_some_and(|rel| batch_key(folder, rel, false).key == key)
            });
    found.files = rest;
    mine
}

impl Intake {
    pub async fn startup(&self) -> Result<(), IntakeError> {
        let _serial = self.serial.lock().await;
        let folders = self.write(|tx, scope| tx.folders(scope).list()).await?;
        let mut found = Vec::new();
        for row in folders.into_iter().filter(|row| row.active) {
            if let Some(files) = self.startup_scan(&row).await? {
                found.push(Found { row, files });
            }
        }
        let open = self
            .write(|tx, scope| tx.intake(scope).open_batches())
            .await?;
        for batch in open.iter().filter(|batch| !is_catch_up(&batch.key)) {
            self.replay(batch, &mut found).await?;
        }
        for folder in found {
            let state = self.folder_state(folder.row.id).await?;
            let members = state.map(|(_, members)| members).unwrap_or_default();
            let mut plan = Plan::new(&folder.row, members);
            for (path, _) in &folder.files {
                plan.detect(self, path, self.now());
            }
            self.apply(plan.actions).await?;
        }
        self.wake_all();
        Ok(())
    }

    async fn startup_scan(
        &self,
        row: &FolderRow,
    ) -> Result<Option<Vec<(WinPath, Timestamp)>>, IntakeError> {
        self.schedule_scan(row);
        if !self.root_present(row).await? {
            return Ok(None);
        }
        if let Err(error) = self.ports.watcher.watch(row.id, &row.path) {
            tracing::warn!(folder = %row.id, ?error, "folder.watchFailed");
        }
        let mut files = Vec::new();
        for entry in self.candidates(row).await? {
            if let Ok(stat) = self.ports.fs.stat(&entry.path).await {
                files.push((entry.path, stat.change_time));
            }
        }
        files.sort_by_key(|(_, changed)| *changed);
        Ok(Some(files))
    }

    async fn replay(&self, batch: &OpenBatchRow, found: &mut [Found]) -> Result<(), IntakeError> {
        let now = self.now();
        let mut folder = found.iter_mut().find(|found| found.row.id == batch.folder);
        let mine = folder
            .as_deref_mut()
            .map(|found| take_key(found, &batch.key))
            .unwrap_or_default();
        let times: Vec<Timestamp> = mine.iter().map(|(_, changed)| *changed).collect();
        let (joined, closing) = open_batch(batch).replay_restart(&times, now);
        if let Some(found) = folder {
            let state = self.folder_state(found.row.id).await?;
            let members = state.map(|(_, members)| members).unwrap_or_default();
            let mut plan = Plan::new(&found.row, members);
            for (path, changed) in mine.iter().take(joined) {
                plan.detect(self, path, *changed);
            }
            self.apply(plan.actions).await?;
            found.files.extend(mine.into_iter().skip(joined));
            found.files.sort_by_key(|(_, changed)| *changed);
        }
        if let Closing::Now(reason) = closing {
            let closure = Closure::of(batch, reason);
            self.write(move |tx, scope| close_series(tx, scope, closure, now))
                .await?;
        }
        Ok(())
    }
}

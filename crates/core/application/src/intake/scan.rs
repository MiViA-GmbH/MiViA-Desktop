use std::collections::{BTreeMap, BTreeSet};

use mivia_domain::ids::{FolderId, Timestamp};
use mivia_domain::intake::is_candidate;
use mivia_ports::fs::{FsError, WalkEntry};
use mivia_ports::store::intake::{FolderChange, FolderRow, KnownPath};

use super::plan::Plan;
use super::{Intake, IntakeError, LOCAL_RECONCILE_MS, NETWORK_RECONCILE_MS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Detect,
    Baseline,
}

fn interval(row: &FolderRow) -> i64 {
    if row.path.is_unc() {
        NETWORK_RECONCILE_MS
    } else {
        LOCAL_RECONCILE_MS
    }
}

fn unchanged(known: &BTreeMap<String, KnownPath>, entry: &WalkEntry) -> bool {
    known
        .get(&entry.path.key())
        .is_some_and(|known| known.size == entry.size && known.mtime == entry.mtime)
}

impl Intake {
    pub(super) fn due_scan(&self) -> Option<FolderId> {
        let now = self.now();
        let schedule = self.schedule.lock();
        schedule
            .due
            .iter()
            .find(|(_, at)| **at <= now)
            .map(|(folder, _)| *folder)
    }

    pub(super) fn next_scan_at(&self) -> Option<Timestamp> {
        self.schedule.lock().due.values().min().copied()
    }

    pub(super) fn schedule_scan(&self, row: &FolderRow) {
        let at = self.now().plus_ms(interval(row));
        self.schedule.lock().due.insert(row.id, at);
    }

    pub(super) async fn reconcile(&self, folder: FolderId) -> Result<(), IntakeError> {
        self.schedule.lock().due.remove(&folder);
        let row = self
            .write(move |tx, scope| tx.folders(scope).get(folder))
            .await?;
        if row.active {
            self.scan(&row, Mode::Detect).await?;
        }
        Ok(())
    }

    pub(super) async fn root_present(&self, row: &FolderRow) -> Result<bool, IntakeError> {
        let present = match self.ports.fs.final_path(&row.path).await {
            Ok(found) => found.key() == row.path.key(),
            Err(FsError::NotFound | FsError::NetworkUnavailable | FsError::NotADirectory) => false,
            Err(error) => return Err(IntakeError::Fs(error)),
        };
        let change = match (present, row.missing_since) {
            (true, Some(_)) => FolderChange::Missing(None),
            (false, None) => FolderChange::Missing(Some(self.now())),
            (true, None) | (false, Some(_)) => return Ok(present),
        };
        if !present {
            self.ports.watcher.unwatch(row.id);
            tracing::warn!(folder = %row.id, "folder.missing");
        }
        let id = row.id;
        self.write(move |tx, scope| tx.folders(scope).update(id, &change))
            .await?;
        Ok(present)
    }

    pub(super) async fn candidates(&self, row: &FolderRow) -> Result<Vec<WalkEntry>, IntakeError> {
        let walk = self
            .ports
            .fs
            .walk(&row.path)
            .await
            .map_err(IntakeError::Fs)?;
        if walk.links_skipped > 0 {
            tracing::info!(folder = %row.id, links = walk.links_skipped, "scan.linksSkipped");
        }
        let folder = row.id;
        let (known, live) = self
            .write(move |tx, scope| {
                let known = tx.intake(scope).known_paths(folder)?;
                let live = tx.intake(scope).live_members(folder)?;
                Ok((known, live))
            })
            .await?;
        let known: BTreeMap<String, KnownPath> = known
            .into_iter()
            .map(|path| (path.path.key(), path))
            .collect();
        let live: BTreeSet<String> = live.iter().map(|member| member.path.key()).collect();
        Ok(walk
            .files
            .into_iter()
            .filter(|entry| {
                row.path
                    .relative_components(&entry.path)
                    .is_some_and(is_candidate)
                    && !live.contains(&entry.path.key())
                    && !unchanged(&known, entry)
            })
            .collect())
    }

    pub(super) async fn scan(&self, row: &FolderRow, mode: Mode) -> Result<usize, IntakeError> {
        self.schedule_scan(row);
        if !self.root_present(row).await? {
            return Ok(0);
        }
        if let Err(error) = self.ports.watcher.watch(row.id, &row.path) {
            tracing::warn!(folder = %row.id, ?error, "folder.watchFailed");
        }
        let found = self.candidates(row).await?;
        let count = found.len();
        match mode {
            Mode::Detect => {
                let state = self.folder_state(row.id).await?;
                let members = state.map(|(_, members)| members).unwrap_or_default();
                let mut plan = Plan::new(row, members);
                for entry in &found {
                    plan.detect(self, &entry.path, self.now());
                }
                self.apply(plan.actions).await?;
            }
            Mode::Baseline => {
                let (folder, now) = (row.id, self.now());
                self.write(move |tx, scope| tx.intake(scope).record_baseline(folder, &found, now))
                    .await?;
            }
        }
        Ok(count)
    }
}

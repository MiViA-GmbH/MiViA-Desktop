use mivia_domain::ids::{FolderId, MINUTE_MS, SECOND_MS, Timestamp};
use mivia_ports::fs::FsError;
use mivia_ports::store::intake::{FolderChange, FolderRow};
use mivia_ports::telemetry::{DriveType, FolderCondition, FolderState};

use super::{Intake, IntakeError};

pub const RETRY_MS: [i64; 4] = [15 * SECOND_MS, 30 * SECOND_MS, MINUTE_MS, 2 * MINUTE_MS];
pub const MISSING_AFTER_MS: i64 = 10 * MINUTE_MS;
const CONFIRM_MS: i64 = 15 * SECOND_MS;

#[derive(Clone, Copy, Debug)]
pub(super) struct Wait {
    clock: Timestamp,
    attempt: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Root {
    Present,
    Gone,
    Unreachable,
}

struct Step {
    change: Option<(FolderChange, FolderState)>,
    retry_ms: Option<i64>,
}

impl Step {
    const fn new(change: Option<(FolderChange, FolderState)>, retry_ms: Option<i64>) -> Self {
        Self { change, retry_ms }
    }
}

fn retry_ms(attempt: usize, until_missing: i64) -> i64 {
    let retry = RETRY_MS
        .get(attempt)
        .or(RETRY_MS.last())
        .copied()
        .unwrap_or(MISSING_AFTER_MS);
    if until_missing > 0 {
        retry.min(until_missing)
    } else {
        retry
    }
}

fn unreachable(wait: &mut Wait, row: &FolderRow, fresh: bool, now: Timestamp) -> Step {
    let waited = now.0 - wait.clock.0;
    let retry = retry_ms(wait.attempt, MISSING_AFTER_MS - waited);
    wait.attempt += 1;
    let change = match (row.missing_since, row.waiting_since) {
        (Some(_), _) if !fresh => None,
        _ if waited >= MISSING_AFTER_MS => {
            Some((FolderChange::Missing(Some(now)), FolderState::Missing))
        }
        (None, Some(_)) if !fresh => None,
        _ => Some((
            FolderChange::Waiting(Some(now)),
            FolderState::WaitingForNetwork,
        )),
    };
    Step::new(change, Some(retry))
}

impl Intake {
    pub(super) async fn root_present(&self, row: &FolderRow) -> Result<bool, IntakeError> {
        let root = match self.ports.fs.final_path(&row.path).await {
            Ok(found) if found.key() == row.path.key() => Root::Present,
            Ok(_) | Err(FsError::NotFound | FsError::NotADirectory) => Root::Gone,
            Err(FsError::NetworkUnavailable) => Root::Unreachable,
            Err(error) => return Err(IntakeError::Fs(error)),
        };
        let now = self.now();
        let step = self.step(row, root, now);
        if let Some(delay) = step.retry_ms {
            self.schedule.lock().due.insert(row.id, now.plus_ms(delay));
        }
        if root != Root::Present {
            self.ports.watcher.unwatch(row.id);
        }
        if let Some((change, state)) = step.change {
            tracing::warn!(folder = %row.id, ?state, "folder.condition");
            self.record_condition(row, state, now);
            let id = row.id;
            self.write(move |tx, scope| tx.folders(scope).update(id, &change))
                .await?;
        }
        Ok(root == Root::Present)
    }

    fn step(&self, row: &FolderRow, root: Root, now: Timestamp) -> Step {
        let mut schedule = self.schedule.lock();
        let unc = row.path.is_unc();
        let missing = Some((FolderChange::Missing(Some(now)), FolderState::Missing));
        match root {
            Root::Present => {
                schedule.waits.remove(&row.id);
                schedule.confirming.remove(&row.id);
                let held = row.missing_since.is_some() || row.waiting_since.is_some();
                Step::new(
                    held.then_some((FolderChange::Missing(None), FolderState::Ok)),
                    None,
                )
            }
            Root::Unreachable if unc => {
                schedule.confirming.remove(&row.id);
                let fresh = !schedule.waits.contains_key(&row.id);
                let wait = schedule.waits.entry(row.id).or_insert(Wait {
                    clock: now,
                    attempt: 0,
                });
                unreachable(wait, row, fresh, now)
            }
            Root::Gone if unc && row.missing_since.is_none() => {
                schedule.waits.remove(&row.id);
                if schedule.confirming.insert(row.id) {
                    return Step::new(None, Some(CONFIRM_MS));
                }
                schedule.confirming.remove(&row.id);
                Step::new(missing, None)
            }
            Root::Gone | Root::Unreachable => {
                Step::new(missing.filter(|_| row.missing_since.is_none()), None)
            }
        }
    }

    fn record_condition(&self, row: &FolderRow, state: FolderState, now: Timestamp) {
        let since = row.waiting_since.or(row.missing_since);
        let duration_ms = since.map(|at| u64::try_from(now.0 - at.0).unwrap_or_default());
        let drive_type = if row.path.is_unc() {
            DriveType::Network
        } else {
            DriveType::Local
        };
        self.ports.telemetry.record(
            FolderCondition {
                folder_id: Some(row.id),
                state: Some(state),
                duration_ms: duration_ms.filter(|_| state == FolderState::Ok),
                drive_type: Some(drive_type),
            }
            .into(),
        );
    }

    pub async fn network_changed(&self, resumed: bool) -> Result<(), IntakeError> {
        let folders = self.write(|tx, scope| tx.folders(scope).list()).await?;
        let now = self.now();
        let mut schedule = self.schedule.lock();
        for wait in schedule.waits.values_mut() {
            *wait = Wait {
                clock: now,
                attempt: 0,
            };
        }
        for row in folders
            .iter()
            .filter(|row| row.active && (resumed || row.path.is_unc()))
        {
            schedule.due.insert(row.id, Timestamp(0));
        }
        drop(schedule);
        self.wakers.intake.notify_one();
        Ok(())
    }

    pub fn check_again(&self, folder: FolderId) {
        self.mark_rescan(folder);
    }
}

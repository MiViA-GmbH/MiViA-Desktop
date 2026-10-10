use mivia_domain::batch::{Closing, ClosureReason, MAX_FILES, OpenBatch};
use mivia_domain::ids::{BatchId, FolderId, Timestamp};
use mivia_domain::intake::{is_catch_up, stem, timer_ms};
use mivia_domain::label::series_label;
use mivia_ports::store::intake::OpenBatchRow;
use mivia_ports::store::{AccountScope, TxError, WriteTx};

use super::{Intake, IntakeError, Tick};

pub(super) fn open_batch(row: &OpenBatchRow) -> OpenBatch {
    OpenBatch {
        opened_at: row.opened_at,
        last_activity: row.last_member_at,
        members: row.files,
        detected: row.detected,
        quiet_ms: timer_ms(row.quiet_ms),
        catch_up: is_catch_up(&row.key),
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Closure {
    pub(super) batch: BatchId,
    pub(super) folder: FolderId,
    pub(super) reason: ClosureReason,
    pub(super) carry_detected: bool,
}

impl Closure {
    pub(super) const fn plain(batch: BatchId, folder: FolderId, reason: ClosureReason) -> Self {
        Self {
            batch,
            folder,
            reason,
            carry_detected: false,
        }
    }

    pub(super) fn of(row: &OpenBatchRow, reason: ClosureReason) -> Self {
        Self {
            carry_detected: reason == ClosureReason::Cap && row.files < MAX_FILES,
            ..Self::plain(row.id, row.folder, reason)
        }
    }
}

pub(super) fn close_series(
    tx: &mut dyn WriteTx,
    scope: AccountScope,
    closure: Closure,
    now: Timestamp,
) -> Result<(), TxError> {
    let Closure { batch, folder, .. } = closure;
    let folder = tx.folders(scope).get(folder)?;
    let paths = tx.intake(scope).member_paths(batch)?;
    let subfolder = paths.first().and_then(|path| {
        let rel = folder.path.relative_components(path)?;
        (rel.len() > 1).then(|| rel.first().cloned()).flatten()
    });
    let stems: Vec<&str> = paths
        .iter()
        .filter_map(|path| path.file_name().map(stem))
        .collect();
    let name = folder.path.file_name().unwrap_or_default();
    let label = series_label(folder.label_rule, subfolder.as_deref(), name, &stems);
    tx.intake(scope).relabel(batch, &label)?;
    tx.batches(scope).close(batch, closure.reason, now)?;
    if closure.carry_detected {
        tx.intake(scope).move_detected(batch, now)?;
    }
    Ok(())
}

impl Intake {
    pub async fn closer_tick(&self) -> Result<Tick, IntakeError> {
        let now = self.now();
        let rows = self
            .write(|tx, scope| tx.intake(scope).open_batches())
            .await?;
        let mut due: Option<Timestamp> = None;
        let mut closing = Vec::new();
        for row in &rows {
            match open_batch(row).closing(now) {
                Closing::Now(reason) => closing.push(Closure::of(row, reason)),
                Closing::DueAt(at) => due = Some(due.map_or(at, |soonest| soonest.min(at))),
            }
        }
        if closing.is_empty() {
            return Ok(due.map_or(Tick::Idle, Tick::DueAt));
        }
        self.write(move |tx, scope| {
            closing
                .into_iter()
                .try_for_each(|target| close_series(tx, scope, target, now))
        })
        .await?;
        self.wakers.readiness.notify_one();
        Ok(Tick::Busy)
    }

    pub async fn finish_series(&self, batch: BatchId) -> Result<(), IntakeError> {
        let now = self.now();
        self.write(move |tx, scope| {
            let row = tx
                .intake(scope)
                .open_batches()?
                .into_iter()
                .find(|row| row.id == batch)
                .ok_or(TxError::NotFound)?;
            close_series(
                tx,
                scope,
                Closure::plain(row.id, row.folder, ClosureReason::Manual),
                now,
            )
        })
        .await?;
        self.wake_all();
        Ok(())
    }
}

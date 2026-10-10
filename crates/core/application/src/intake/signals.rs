use std::collections::BTreeMap;

use mivia_domain::batch::{ClosureReason, MAX_FILES};
use mivia_domain::ids::{FolderId, Timestamp};
use mivia_domain::image_format::has_accepted_extension;
use mivia_domain::path::WinPath;
use mivia_domain::readiness::QUIET_FLOOR_MS;
use mivia_domain::source_file::SourceFileState;
use mivia_ports::store::intake::{FolderRow, Member};
use mivia_ports::store::{AccountScope, TxError, WriteTx};
use mivia_ports::watch::{FsSignal, SignalKind};

use super::closer::{Closure, close_series};
use super::plan::{Action, Op, Plan};
use super::{Intake, IntakeError, SIGNAL_CHUNK, Tick, tolerate};

impl Intake {
    pub async fn intake_tick(&self) -> Result<Tick, IntakeError> {
        let _serial = self.serial.lock().await;
        for folder in self.sink.take_overflow() {
            self.mark_rescan(folder);
        }
        let ops = self.drain().await;
        if !ops.is_empty() {
            self.handle(ops).await?;
            return Ok(Tick::Busy);
        }
        if let Some(folder) = self.due_scan() {
            self.reconcile(folder).await?;
            return Ok(Tick::Busy);
        }
        Ok(self.next_scan_at().map_or(Tick::Idle, Tick::DueAt))
    }

    async fn drain(&self) -> Vec<(FolderId, Op)> {
        let mut receiver = self.signals.lock().await;
        let mut ops = Vec::new();
        let mut schedule = self.schedule.lock();
        while ops.len() < SIGNAL_CHUNK {
            let Ok(signal) = receiver.try_recv() else {
                break;
            };
            let pending = schedule.rename_from.take();
            pair(signal, pending, &mut ops, &mut schedule);
        }
        if ops.is_empty()
            && let Some(FsSignal {
                folder,
                kind: SignalKind::RenamedFrom(path),
            }) = schedule.rename_from.take()
        {
            ops.push((folder, Op::Remove(path)));
        }
        ops
    }

    async fn handle(&self, ops: Vec<(FolderId, Op)>) -> Result<(), IntakeError> {
        let mut grouped: BTreeMap<FolderId, Vec<Op>> = BTreeMap::new();
        for (folder, op) in ops {
            grouped.entry(folder).or_default().push(op);
        }
        for (folder, ops) in grouped {
            let Some((row, members)) = self.folder_state(folder).await? else {
                continue;
            };
            let mut plan = Plan::new(&row, members);
            for op in ops {
                self.plan_op(&mut plan, op).await;
            }
            self.apply(plan.actions).await?;
        }
        Ok(())
    }

    pub(super) async fn folder_state(
        &self,
        folder: FolderId,
    ) -> Result<Option<(FolderRow, Vec<Member>)>, IntakeError> {
        self.write(move |tx, scope| {
            let row = match tx.folders(scope).get(folder) {
                Ok(row) if row.active && row.missing_since.is_none() => row,
                Ok(_) | Err(TxError::NotFound) => return Ok(None),
                Err(error) => return Err(error),
            };
            let members = tx.intake(scope).live_members(folder)?;
            Ok(Some((row, members)))
        })
        .await
    }

    async fn plan_op(&self, plan: &mut Plan<'_>, op: Op) {
        match op {
            Op::Touch(path) => self.touch(plan, &path).await,
            Op::Remove(path) if path == plan.folder.path => self.mark_rescan(plan.folder.id),
            Op::Remove(path) => plan.remove(&path),
            Op::Rename(from, to) => {
                if !plan.rename(&from, &to) {
                    self.touch(plan, &to).await;
                }
            }
        }
    }

    async fn member_touched(&self, member: &Member) -> Option<Action> {
        if member.state == SourceFileState::Detected {
            return Some(Action::Bump(member.id));
        }
        let stat = self.ports.fs.stat(&member.path).await.ok()?;
        (stat.size != member.size || stat.mtime != member.mtime)
            .then_some(Action::Changed(member.id))
    }

    async fn touch(&self, plan: &mut Plan<'_>, path: &WinPath) {
        if let Some(member) = plan.member_at(path).cloned() {
            let action = self.member_touched(&member).await;
            plan.actions.extend(action);
            return;
        }
        let name = path.file_name().unwrap_or_default();
        if has_accepted_extension(name) {
            plan.detect(self, path, self.now());
            return;
        }
        if self.ports.fs.stat(path).await.is_ok_and(|stat| stat.is_dir)
            && let Ok(walk) = self.ports.fs.walk(path).await
        {
            for file in walk.files {
                plan.detect(self, &file.path, self.now());
            }
        }
    }

    pub(super) async fn apply(&self, actions: Vec<Action>) -> Result<(), IntakeError> {
        if actions.is_empty() {
            return Ok(());
        }
        let now = self.now();
        self.write(move |tx, scope| {
            actions
                .into_iter()
                .try_for_each(|action| tolerate(perform(tx, scope, action, now)))
        })
        .await?;
        self.wake_all();
        Ok(())
    }

    pub(super) fn mark_rescan(&self, folder: FolderId) {
        self.schedule.lock().due.insert(folder, Timestamp(0));
        self.wakers.intake.notify_one();
    }
}

fn pair(
    signal: FsSignal,
    pending: Option<FsSignal>,
    ops: &mut Vec<(FolderId, Op)>,
    schedule: &mut super::Schedule,
) {
    let folder = signal.folder;
    if let SignalKind::RenamedTo(to) = signal.kind {
        match pending {
            Some(FsSignal {
                folder: from_folder,
                kind: SignalKind::RenamedFrom(from),
            }) if from_folder == folder => ops.push((folder, Op::Rename(from, to))),
            Some(other) => {
                flush(other, ops);
                ops.push((folder, Op::Touch(to)));
            }
            None => ops.push((folder, Op::Touch(to))),
        }
        return;
    }
    if let Some(other) = pending {
        flush(other, ops);
    }
    match signal.kind {
        SignalKind::RenamedFrom(_) => schedule.rename_from = Some(signal),
        SignalKind::Touched(path) | SignalKind::RenamedTo(path) => {
            ops.push((folder, Op::Touch(path)));
        }
        SignalKind::Removed(path) => ops.push((folder, Op::Remove(path))),
        SignalKind::Rescan => {
            schedule.due.insert(folder, Timestamp(0));
        }
    }
}

fn flush(signal: FsSignal, ops: &mut Vec<(FolderId, Op)>) {
    if let SignalKind::RenamedFrom(path) = signal.kind {
        ops.push((signal.folder, Op::Remove(path)));
    }
}

fn perform(
    tx: &mut dyn WriteTx,
    scope: AccountScope,
    action: Action,
    now: Timestamp,
) -> Result<(), TxError> {
    let intake = tx.intake(scope);
    let floor = now.plus_ms(QUIET_FLOOR_MS);
    match action {
        Action::Detect(detection) => {
            let detected = intake.detect(&detection)?;
            if detected.files < MAX_FILES {
                return Ok(());
            }
            let full = Closure::plain(detected.batch, detection.folder, ClosureReason::Cap);
            close_series(tx, scope, full, now)
        }
        Action::Bump(id) => intake.touch(id, floor),
        Action::Changed(id) => intake
            .content_changed(id, now)
            .and_then(|()| intake.touch(id, floor)),
        Action::Remove(id) => intake.remove(id, now),
        Action::Rebase(id, path, label) => intake.rebase(id, &path, &label),
        Action::Rekey(batch, key, label) => intake.rekey(batch, &key, &label),
    }
}

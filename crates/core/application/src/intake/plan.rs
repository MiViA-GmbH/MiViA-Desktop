use std::collections::BTreeSet;

use mivia_domain::ids::{BatchId, SourceFileId, Timestamp};
use mivia_domain::intake::{BatchKey, batch_key, is_candidate, stem};
use mivia_domain::label::{member_label, series_label};
use mivia_domain::path::WinPath;
use mivia_domain::readiness::QUIET_FLOOR_MS;
use mivia_ports::store::intake::{Detection, FolderRow, Member};

use super::Intake;

pub(super) enum Op {
    Touch(WinPath),
    Remove(WinPath),
    Rename(WinPath, WinPath),
}

pub(super) enum Action {
    Detect(Detection),
    Bump(SourceFileId),
    Changed(SourceFileId),
    Remove(SourceFileId),
    Rebase(SourceFileId, WinPath, String),
    Rekey(BatchId, BatchKey, String),
}

#[derive(Clone, Copy)]
enum Arrival {
    Live(Timestamp),
    CatchUp,
}

pub(super) struct Plan<'a> {
    pub(super) folder: &'a FolderRow,
    members: Vec<Member>,
    detected: BTreeSet<String>,
    pub(super) actions: Vec<Action>,
}

fn relative(root: &WinPath, path: &WinPath) -> Option<Vec<String>> {
    root.relative_components(path)
        .filter(|rest| !rest.is_empty())
        .map(<[String]>::to_vec)
}

fn rebased(from: &WinPath, to: &WinPath, path: &WinPath) -> Option<WinPath> {
    from.relative_components(path)?
        .iter()
        .try_fold(to.clone(), |joined, part| joined.join(part).ok())
}

impl<'a> Plan<'a> {
    pub(super) fn new(folder: &'a FolderRow, members: Vec<Member>) -> Self {
        Self {
            folder,
            members,
            detected: BTreeSet::new(),
            actions: Vec::new(),
        }
    }

    pub(super) fn member_at(&self, path: &WinPath) -> Option<&Member> {
        let key = path.key();
        self.members.iter().find(|member| member.path.key() == key)
    }

    pub(super) fn detect(&mut self, intake: &Intake, path: &WinPath, activity: Timestamp) {
        self.detect_keyed(intake, path, Arrival::Live(activity));
    }

    pub(super) fn detect_catch_up(&mut self, intake: &Intake, path: &WinPath) {
        self.detect_keyed(intake, path, Arrival::CatchUp);
    }

    fn detect_keyed(&mut self, intake: &Intake, path: &WinPath, arrival: Arrival) {
        let (activity, catch_up) = match arrival {
            Arrival::Live(at) => (at, false),
            Arrival::CatchUp => (intake.now(), true),
        };
        let Some(rel) = relative(&self.folder.path, path) else {
            return;
        };
        if !is_candidate(&rel) || self.member_at(path).is_some() {
            return;
        }
        if !self.detected.insert(path.key()) {
            return;
        }
        let key = batch_key(self.folder.id, &rel, catch_up);
        let name = self.folder.path.file_name().unwrap_or_default();
        let file = rel.last().map(String::as_str).unwrap_or_default();
        self.actions.push(Action::Detect(Detection {
            series_label: series_label(
                self.folder.label_rule,
                key.subfolder.as_deref(),
                name,
                &[stem(file)],
            ),
            key,
            folder: self.folder.id,
            label: member_label(&rel),
            path: path.clone(),
            next_attempt_at: intake.now().plus_ms(QUIET_FLOOR_MS),
            activity,
            app_version: intake.app_version.clone(),
        }));
    }

    pub(super) fn remove(&mut self, path: &WinPath) {
        let gone = self
            .members
            .iter()
            .filter(|member| path.contains(&member.path))
            .map(|member| member.id)
            .collect::<Vec<_>>();
        self.members.retain(|member| !gone.contains(&member.id));
        self.actions.extend(gone.into_iter().map(Action::Remove));
    }

    pub(super) fn rename(&mut self, from: &WinPath, to: &WinPath) -> bool {
        let top_level = relative(&self.folder.path, from).is_some_and(|rel| rel.len() == 1);
        let mut moved = false;
        let mut rekeyed = BTreeSet::new();
        for member in &mut self.members {
            let Some(path) = rebased(from, to, &member.path) else {
                continue;
            };
            moved = true;
            let rel = relative(&self.folder.path, &path).unwrap_or_default();
            if !is_candidate(&rel) {
                self.actions.push(Action::Remove(member.id));
                continue;
            }
            let key = batch_key(self.folder.id, &rel, false);
            if top_level && rel.len() > 1 && rekeyed.insert(member.batch_id) {
                let label = key.subfolder.clone().unwrap_or_default();
                self.actions
                    .push(Action::Rekey(member.batch_id, key, label));
            }
            self.actions
                .push(Action::Rebase(member.id, path.clone(), member_label(&rel)));
            member.path = path;
        }
        moved
    }
}

use mivia_ports::ipc::change::{Change, StoreChanged};
use mivia_ports::store::Committed;

pub const MAX_IDS: usize = 200;

#[derive(Debug, Default)]
pub struct Coalescer {
    from: Option<i64>,
    to: i64,
    changes: Vec<Change>,
}

const fn carries_id(change: &Change) -> bool {
    matches!(
        change,
        Change::BatchSummary { .. }
            | Change::BatchItems { .. }
            | Change::Folder { .. }
            | Change::Reports { .. }
    )
}

fn same_slot(left: &Change, right: &Change) -> bool {
    match (left, right) {
        (Change::BatchSummary { batch_id: a, .. }, Change::BatchSummary { batch_id: b, .. }) => {
            a == b
        }
        _ => left == right,
    }
}

impl Coalescer {
    pub fn push(&mut self, committed: &Committed) {
        self.from.get_or_insert(committed.revision - 1);
        self.to = committed.revision;
        if self.changes == [Change::All] {
            return;
        }
        for change in &committed.changes {
            self.merge(change.clone());
        }
        let ids = self
            .changes
            .iter()
            .filter(|change| carries_id(change))
            .count();
        if ids > MAX_IDS || self.changes.contains(&Change::All) {
            self.changes = vec![Change::All];
        }
    }

    fn merge(&mut self, change: Change) {
        match self
            .changes
            .iter_mut()
            .find(|known| same_slot(known, &change))
        {
            Some(known) => *known = change,
            None => self.changes.push(change),
        }
    }

    pub fn take(&mut self) -> Option<StoreChanged> {
        let from = self.from.take()?;
        Some(StoreChanged {
            from_revision: from.to_string(),
            to_revision: self.to.to_string(),
            changes: std::mem::take(&mut self.changes),
        })
    }
}

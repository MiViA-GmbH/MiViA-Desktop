use std::collections::{BTreeMap, BTreeSet};

use mivia_domain::ids::BatchId;
use mivia_ports::store::TxError;
use mivia_ports::store::pipeline::{Fairness, SubmitRow, UploadRow};

const ROUNDS: usize = 4;

pub(super) trait Fair {
    fn fairness(&self) -> Fairness;
}

impl Fair for UploadRow {
    fn fairness(&self) -> Fairness {
        self.fairness
    }
}

impl Fair for SubmitRow {
    fn fairness(&self) -> Fairness {
        self.fairness
    }
}

pub(super) fn gather<T: Fair>(
    mut fetch: impl FnMut(&[BatchId]) -> Result<Vec<T>, TxError>,
) -> Result<Vec<T>, TxError> {
    let mut rows = Vec::new();
    let mut skip = Vec::new();
    for _ in 0..ROUNDS {
        let found = fetch(&skip)?;
        if found.is_empty() {
            break;
        }
        let batches: BTreeSet<BatchId> = found.iter().map(|row| row.fairness().batch).collect();
        skip.extend(batches);
        rows.extend(found);
    }
    Ok(interleave(rows))
}

pub(super) fn interleave<T: Fair>(rows: Vec<T>) -> Vec<T> {
    let mut seen: BTreeMap<BatchId, usize> = BTreeMap::new();
    let mut ranked: Vec<(usize, T)> = rows
        .into_iter()
        .map(|row| {
            let rank = seen.entry(row.fairness().batch).or_default();
            *rank += 1;
            (*rank, row)
        })
        .collect();
    ranked.sort_by_key(|(rank, row)| {
        let fairness = row.fairness();
        (
            !fairness.one_off,
            *rank,
            fairness.opened_at,
            fairness.arrival_seq,
        )
    });
    ranked.into_iter().map(|(_, row)| row).collect()
}

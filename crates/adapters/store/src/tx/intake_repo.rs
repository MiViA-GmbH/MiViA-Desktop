use mivia_domain::ids::{BatchId, FolderId, SourceFileId, Timestamp};
use mivia_domain::intake::BatchKey;
use mivia_domain::path::WinPath;
use mivia_domain::readiness::Attempts;
use mivia_ports::fs::{ImageRead, WalkEntry};
use mivia_ports::store::TxError;
use mivia_ports::store::intake::{
    Classified, Detected, Detection, IntakeRepo, KnownPath, Member, OpenBatchRow, ProbeRow,
    Sighting,
};
use rusqlite::{OptionalExtension, params};

use super::SqlTx;
use crate::errors::SqlResult;

impl IntakeRepo for SqlTx<'_> {
    fn live_members(&mut self, folder: FolderId) -> Result<Vec<Member>, TxError> {
        self.live_members_of(folder)
    }

    fn detect(&mut self, detection: &Detection) -> Result<Detected, TxError> {
        self.detect_file(detection)
    }

    fn touch(&mut self, id: SourceFileId, next_attempt_at: Timestamp) -> Result<(), TxError> {
        self.tx
            .prepare_cached(
                "UPDATE source_file SET next_attempt_at = ?3, attempts = 0 \
                 WHERE id = ?1 AND account_id = ?2 AND state = 'detected'",
            )
            .and_then(|mut statement| {
                statement.execute(params![id.0, self.account(), next_attempt_at.0])
            })
            .tx()
            .map(drop)
    }

    fn rebase(&mut self, id: SourceFileId, path: &WinPath, label: &str) -> Result<(), TxError> {
        self.tx
            .execute(
                "UPDATE source_file SET path = ?3, label = ?4 WHERE id = ?1 AND account_id = ?2",
                params![id.0, self.account(), path.to_string(), label],
            )
            .tx()
            .map(drop)
    }

    fn remove(&mut self, id: SourceFileId, now: Timestamp) -> Result<(), TxError> {
        self.remove_file(id, now)
    }

    fn content_changed(&mut self, id: SourceFileId, now: Timestamp) -> Result<(), TxError> {
        self.change_file(id, now)
    }

    fn claim_probes(
        &mut self,
        now: Timestamp,
        exclude: &[SourceFileId],
        limit: u32,
    ) -> Result<Vec<ProbeRow>, TxError> {
        self.claim_probe_rows(now, exclude, limit)
    }

    fn next_probe_at(&mut self) -> Result<Option<Timestamp>, TxError> {
        self.tx
            .prepare_cached(crate::sql::NEXT_PROBE)
            .and_then(|mut statement| {
                statement
                    .query_row([self.account()], |row| row.get::<_, Option<i64>>(0))
                    .optional()
            })
            .tx()
            .map(|found| found.flatten().map(Timestamp))
    }

    fn retry(
        &mut self,
        id: SourceFileId,
        at: Timestamp,
        attempts: Attempts,
    ) -> Result<(), TxError> {
        self.set_retry(id, at, attempts)
    }

    fn ready(
        &mut self,
        id: SourceFileId,
        read: &ImageRead,
        now: Timestamp,
    ) -> Result<Sighting, TxError> {
        self.mark_ready(id, read, now)
    }

    fn classify(
        &mut self,
        id: SourceFileId,
        outcome: Classified,
        now: Timestamp,
    ) -> Result<(), TxError> {
        self.classify_file(id, outcome, now)
    }

    fn known_paths(&mut self, folder: FolderId) -> Result<Vec<KnownPath>, TxError> {
        self.known_paths_of(folder)
    }

    fn record_baseline(
        &mut self,
        folder: FolderId,
        files: &[WalkEntry],
        now: Timestamp,
    ) -> Result<(), TxError> {
        self.insert_baseline(folder, files, now)
    }

    fn take_baseline(
        &mut self,
        folder: FolderId,
        since: Timestamp,
    ) -> Result<Vec<WinPath>, TxError> {
        self.drain_baseline(folder, since)
    }

    fn open_batches(&mut self) -> Result<Vec<OpenBatchRow>, TxError> {
        self.open_batch_rows()
    }

    fn member_paths(&mut self, batch: BatchId) -> Result<Vec<WinPath>, TxError> {
        self.paths_in(batch)
    }

    fn relabel(&mut self, batch: BatchId, label: &str) -> Result<(), TxError> {
        self.set_label(batch, label)
    }

    fn rekey(&mut self, batch: BatchId, key: &BatchKey, label: &str) -> Result<(), TxError> {
        self.set_key(batch, key, label)
    }

    fn move_detected(&mut self, from: BatchId, now: Timestamp) -> Result<(), TxError> {
        self.carry_detected(from, now)
    }
}

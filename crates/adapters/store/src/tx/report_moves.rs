use mivia_domain::ids::{BatchId, Fingerprint, ReportId, ReportPartId, Timestamp};
use mivia_domain::path::WinPath;
use mivia_ports::ipc::change::Change;
use mivia_ports::store::TxError;
use mivia_ports::store::history::{MovablePart, MoveSet, Moved, NewEvent, Trigger};
use rusqlite::{OptionalExtension, params};

use super::SqlTx;
use super::counters::Counts;
use super::history::json;
use super::report_parts::language_of;
use crate::codec::parse_path;
use crate::errors::SqlResult;

const MOVE_HEAD: &str = "SELECT r.batch_id, r.language, r.replaced_by, \
     (SELECT count(*) FROM report_part x WHERE x.report_id = r.replaced_by AND x.state <> 'saved'), \
     (SELECT count(*) FROM report_part x WHERE x.report_id = r.replaced_by), \
     EXISTS (SELECT 1 FROM report_part x WHERE x.report_id = r.id \
     AND x.moved_to_replaced_at IS NOT NULL), \
     (SELECT count(*) FROM report_part x WHERE x.report_id = r.id AND x.state <> 'saved') \
     FROM report r WHERE r.id = ?1 AND r.account_id = ?2";

const MOVE_PARTS: &str = "SELECT id, file_path, size, sha256 FROM report_part \
     WHERE report_id = ?1 AND state = 'saved' AND file_path IS NOT NULL \
     ORDER BY format DESC, part_no";

const SETTLE: &str = "UPDATE batch SET needs_attention = 0, attention_cleared_at = ?2, \
     attention_cleared_how = ?3 WHERE id = ?1 AND needs_attention = 1 \
     AND problems = 0 AND reports_problem = 0 AND (?4 OR reports_stale = 0)";

type MoveHead = (Option<i64>, String, Option<i64>, i64, i64, bool, i64);

impl SqlTx<'_> {
    pub(super) fn settle(
        &mut self,
        batch: Option<i64>,
        now: Timestamp,
        keep_stale: bool,
    ) -> Result<(), TxError> {
        let Some(batch) = batch else {
            return Ok(());
        };
        let how = if keep_stale { "dismissed" } else { "opened" };
        let changed = self
            .tx
            .execute(SETTLE, params![batch, now.0, how, keep_stale])
            .tx()?;
        if changed > 0 {
            self.record(Change::NeedsAttention);
        }
        Ok(())
    }

    pub(super) fn keep_report(
        &mut self,
        id: ReportId,
        now: Timestamp,
    ) -> Result<Option<BatchId>, TxError> {
        let batch: Option<Option<i64>> = self
            .tx
            .query_row(
                "UPDATE report SET attention_cleared_at = ?3 WHERE id = ?1 AND account_id = ?2 \
                 AND stale = 1 RETURNING batch_id",
                params![id.0, self.account(), now.0],
                |row| row.get(0),
            )
            .optional()
            .tx()?;
        let batch = batch.ok_or(TxError::NotFound)?;
        self.settle(batch, now, true)?;
        let mut event = NewEvent::new("attention_cleared", Trigger::User, now);
        event.batch = batch.map(BatchId);
        event.report = Some(id);
        event.after = Some(json(&[("how", "kept".to_owned())]));
        self.log_event(&event)?;
        self.record(Change::History);
        Ok(batch.map(BatchId))
    }

    pub(super) fn read_move_set(&self, id: ReportId) -> Result<MoveSet, TxError> {
        let head: MoveHead = self
            .tx
            .query_row(MOVE_HEAD, [id.0, self.account()], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            })
            .tx()?;
        let (batch, language, replaced_by, unsaved_new, total_new, moved, unsaved) = head;
        let raws: Vec<(i64, String, i64, Vec<u8>)> = self.rows(MOVE_PARTS, [id.0], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?;
        let parts = raws
            .into_iter()
            .map(|(part, path, size, sha)| {
                let sha: [u8; 32] = sha
                    .try_into()
                    .map_err(|_| TxError::Corrupt("part hash".to_owned()))?;
                Ok(MovablePart {
                    id: ReportPartId(part),
                    path: parse_path(&path)?,
                    size: u64::try_from(size).unwrap_or_default(),
                    sha256: Fingerprint(sha),
                })
            })
            .collect::<Result<Vec<_>, TxError>>()?;
        Ok(MoveSet {
            batch: batch.map(BatchId),
            language: language_of(&language)?,
            replacement_saved: replaced_by.is_some() && unsaved_new == 0 && total_new > 0,
            moved,
            parts,
            unsaved: u32::try_from(unsaved).unwrap_or(u32::MAX),
        })
    }

    pub(super) fn record_move(
        &mut self,
        part: ReportPartId,
        path: &WinPath,
        how: Moved,
    ) -> Result<(), TxError> {
        let (batch, report, before): (Option<i64>, i64, Option<String>) = self
            .tx
            .query_row(
                "SELECT r.batch_id, r.id, p.file_path FROM report_part p \
                 JOIN report r ON r.id = p.report_id WHERE p.id = ?1 AND r.account_id = ?2",
                [part.0, self.account()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .tx()?;
        self.run(
            "UPDATE report_part SET saved_path = COALESCE(saved_path, file_path), file_path = ?2, \
             moved_to_replaced_at = ?3 WHERE id = ?1",
            params![part.0, path.to_string(), moved_at(how)],
        )?;
        let (action, at) = match how {
            Moved::ToReplaced(at) => ("moved_to_replaced", at),
            Moved::Restored(at) => ("restored_from_replaced", at),
        };
        let mut event = NewEvent::new(action, Trigger::User, at);
        event.batch = batch.map(BatchId);
        event.report = Some(ReportId(report));
        event.before = Some(json(&[("path", before.unwrap_or_default())]));
        event.after = Some(json(&[("path", path.to_string())]));
        self.log_event(&event)?;
        self.reports_changed(batch, (Counts::default(), Counts::default()), at)?;
        Ok(())
    }
}

const fn moved_at(how: Moved) -> Option<i64> {
    match how {
        Moved::ToReplaced(at) => Some(at.0),
        Moved::Restored(_) => None,
    }
}

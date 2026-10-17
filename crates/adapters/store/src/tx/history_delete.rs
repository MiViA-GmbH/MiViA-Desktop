use mivia_domain::batch::{ClosureReason, closed_sort_at};
use mivia_domain::ids::{BatchId, Fingerprint, ReportPartId, SourceFileId, Timestamp};
use mivia_domain::natural::sort_key;
use mivia_domain::source_file::SourceFileEvent;
use mivia_ports::ipc::change::Change;
use mivia_ports::store::history::{
    DeleteRequest, DeleteTarget, Deletion, MovablePart, NewEvent, Trigger,
};
use mivia_ports::store::{BatchRepo, SourceFileRepo, TxError};
use rusqlite::{OptionalExtension, params};

use super::batches::write_search_doc;
use super::history::json;
use super::{SqlTx, id_list};
use crate::codec::parse_path;
use crate::errors::SqlResult;
use crate::sql;

const LIVE_BATCH: &str = "SELECT open, label FROM batch \
     WHERE id = ?1 AND account_id = ?2 AND deleted_at IS NULL";

const IMAGE_LABEL: &str =
    "SELECT label FROM source_file WHERE id = ?1 AND batch_id = ?2 AND account_id = ?3";

const FILES: &str = "SELECT id FROM source_file WHERE batch_id = ?1 ORDER BY arrival_seq";

const JOBS: &str = "SELECT DISTINCT x.job FROM (SELECT c.id AS origin, c.server_job_id AS job \
     FROM computation c WHERE c.source_file_id IN (SELECT value FROM json_each(?1)) \
     AND c.server_job_id IS NOT NULL UNION SELECT o.id, o.server_job_id FROM computation c \
     JOIN computation o ON o.id = c.reused_from JOIN batch ob ON ob.id = o.batch_id \
     WHERE c.source_file_id IN (SELECT value FROM json_each(?1)) \
     AND o.server_job_id IS NOT NULL AND ob.deleted_at IS NOT NULL) x \
     WHERE NOT EXISTS (SELECT 1 FROM computation u JOIN batch ub ON ub.id = u.batch_id \
     WHERE u.reused_from = x.origin AND ub.deleted_at IS NULL \
     AND u.source_file_id NOT IN (SELECT value FROM json_each(?1))) ORDER BY x.job";

const IMAGES: &str = "SELECT DISTINCT s.server_image_id FROM source_file s \
     WHERE s.id IN (SELECT value FROM json_each(?1)) AND s.server_image_id IS NOT NULL \
     AND NOT EXISTS (SELECT 1 FROM source_file o JOIN batch b ON b.id = o.batch_id \
     WHERE o.account_id = s.account_id AND o.server_image_id = s.server_image_id \
     AND b.deleted_at IS NULL AND o.id NOT IN (SELECT value FROM json_each(?1))) \
     ORDER BY s.server_image_id";

const RELEASE_LEDGER: &str = "DELETE FROM ledger_entry WHERE computation_id IN \
     (SELECT id FROM computation WHERE source_file_id IN (SELECT value FROM json_each(?1)))";

const MEMBER_REPORTS: &str = "SELECT DISTINCT r.id FROM computation c \
     JOIN report_member m ON m.computation_id = c.id JOIN report r ON r.id = m.report_id \
     WHERE c.source_file_id IN (SELECT value FROM json_each(?1)) AND r.deleted_at IS NULL";

const BATCH_REPORTS: &str =
    "SELECT id FROM report WHERE batch_id = ?1 AND deleted_at IS NULL ORDER BY id";

const SAVED_PARTS: &str = "SELECT id, file_path, size, sha256 FROM report_part \
     WHERE report_id IN (SELECT value FROM json_each(?1)) AND state = 'saved' \
     AND file_path IS NOT NULL AND size IS NOT NULL AND sha256 IS NOT NULL ORDER BY id";

const HIDE_REPORTS: &str = "UPDATE report SET deleted_at = ?2 \
     WHERE id IN (SELECT value FROM json_each(?1))";

const STOP_PARTS: &str = "UPDATE report_part SET state = 'failed', error = 'discarded' \
     WHERE report_id IN (SELECT value FROM json_each(?1)) AND state IN ('requested', 'not_saved')";

const TRASH: &str = "INSERT INTO batch (account_id, origin, batch_key, label, label_sort, open, \
     opened_at, closed_at, sort_at, report_settings, app_version, deleted_at, delete_on_server) \
     SELECT account_id, 'one_off', 'deleted', '', ?4, 0, ?2, ?2, ?3, '{}', app_version, ?2, ?5 \
     FROM batch WHERE id = ?1 RETURNING id";

const RECOUNT: &str = "UPDATE batch SET \
     files = (SELECT count(*) FROM source_file WHERE batch_id = ?1), \
     done = (SELECT count(*) FROM computation WHERE batch_id = ?1 AND state = 'done'), \
     failed = (SELECT count(*) FROM computation WHERE batch_id = ?1 AND state = 'failed'), \
     problems = (SELECT count(*) FROM source_file WHERE batch_id = ?1 \
     AND state IN ('rejected', 'unreadable')) + (SELECT count(*) FROM computation \
     WHERE batch_id = ?1 AND state IN ('failed', 'unavailable', 'lost')), \
     active_count = (SELECT count(*) FROM computation WHERE batch_id = ?1 \
     AND state IN ('queued', 'submitting', 'computing')), \
     reports_total = (SELECT count(*) FROM report_part p JOIN report r ON r.id = p.report_id \
     WHERE r.batch_id = ?1 AND r.deleted_at IS NULL), \
     reports_saved = (SELECT count(*) FROM report_part p JOIN report r ON r.id = p.report_id \
     WHERE r.batch_id = ?1 AND r.deleted_at IS NULL AND p.state = 'saved'), \
     reports_problem = (SELECT count(*) FROM report_part p JOIN report r ON r.id = p.report_id \
     WHERE r.batch_id = ?1 AND r.deleted_at IS NULL AND p.state IN ('not_saved', 'failed')), \
     reports_stale = (SELECT count(*) FROM report_part p JOIN report r ON r.id = p.report_id \
     WHERE r.batch_id = ?1 AND r.deleted_at IS NULL AND r.stale = 1 AND r.replaced_by IS NULL) \
     WHERE id = ?1";

type RawPart = (i64, String, i64, Vec<u8>);

fn movable(raw: RawPart) -> Result<MovablePart, TxError> {
    let (id, path, size, sha) = raw;
    let sha: [u8; 32] = sha
        .try_into()
        .map_err(|_| TxError::Corrupt("part hash".to_owned()))?;
    Ok(MovablePart {
        id: ReportPartId(id),
        path: parse_path(&path)?,
        size: u64::try_from(size).unwrap_or_default(),
        sha256: Fingerprint(sha),
    })
}

fn deleted_event(
    request: DeleteRequest,
    deletion: &Deletion,
    label: String,
    now: Timestamp,
) -> NewEvent {
    let what = match request.target {
        DeleteTarget::Series(_) => "series",
        DeleteTarget::Image(..) => "image",
    };
    let mut event = NewEvent::new("deleted", Trigger::User, now);
    event.batch = Some(request.target.batch());
    event.after = Some(json(&[
        ("what", what.to_owned()),
        ("label", label),
        ("images", deletion.images.to_string()),
        ("reportFiles", deletion.parts.len().to_string()),
        ("server", request.server.to_string()),
    ]));
    event
}

impl SqlTx<'_> {
    fn live_batch(&self, batch: BatchId) -> Result<(bool, String), TxError> {
        let found = self
            .tx
            .query_row(LIVE_BATCH, [batch.0, self.account()], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .optional()
            .tx()?;
        found.ok_or(TxError::NotFound)
    }

    fn image_label(&self, batch: BatchId, file: SourceFileId) -> Result<String, TxError> {
        let found = self
            .tx
            .query_row(IMAGE_LABEL, [file.0, batch.0, self.account()], |row| {
                row.get(0)
            })
            .optional()
            .tx()?;
        found.ok_or(TxError::NotFound)
    }

    fn stop_work(&mut self, files: &[i64], now: Timestamp) -> Result<(), TxError> {
        for file in files {
            let id = SourceFileId(*file);
            match SourceFileRepo::apply(self, id, SourceFileEvent::Removed, now) {
                Ok(_) | Err(TxError::Illegal(_)) => {}
                Err(other) => return Err(other),
            }
            self.cancel_work(id, now)?;
        }
        Ok(())
    }

    fn saved_parts(&self, reports: &str) -> Result<Vec<MovablePart>, TxError> {
        let raws: Vec<RawPart> = self.rows(SAVED_PARTS, [reports], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?;
        raws.into_iter().map(movable).collect()
    }

    fn hide_reports(
        &self,
        reports: &[i64],
        files: bool,
        now: Timestamp,
    ) -> Result<Vec<MovablePart>, TxError> {
        let list = id_list(reports, |id| id);
        let parts = if files {
            self.saved_parts(&list)?
        } else {
            Vec::new()
        };
        self.run(HIDE_REPORTS, params![list, now.0])?;
        self.run(STOP_PARTS, [&list])?;
        Ok(parts)
    }

    fn trash_image(
        &self,
        (batch, file): (BatchId, SourceFileId),
        server: bool,
        now: Timestamp,
    ) -> Result<(), TxError> {
        let latest: Option<i64> = self
            .tx
            .query_row("SELECT max(sort_at) FROM batch WHERE open = 0", [], |row| {
                row.get(0)
            })
            .tx()?;
        let sort_at = closed_sort_at(now, latest);
        let trash: i64 = self
            .tx
            .query_row(
                TRASH,
                params![batch.0, now.0, sort_at, sort_key(""), server],
                |row| row.get(0),
            )
            .tx()?;
        self.run(sql::MOVE_MEMBER, [file.0, trash])?;
        self.run(
            "UPDATE computation SET batch_id = ?2 WHERE source_file_id = ?1",
            [file.0, trash],
        )?;
        self.run(RECOUNT, [batch.0])?;
        self.run(RECOUNT, [trash])?;
        write_search_doc(self.tx, batch.0).tx()
    }

    fn trash_series(&self, batch: BatchId) -> Result<(), TxError> {
        let sort_at: i64 = self
            .tx
            .query_row(
                "SELECT sort_at FROM batch WHERE id = ?1",
                [batch.0],
                |row| row.get(0),
            )
            .tx()?;
        self.run("DELETE FROM batch_search WHERE rowid = ?1", [sort_at])
    }

    fn affected_reports(&self, request: DeleteRequest, files: &str) -> Result<Vec<i64>, TxError> {
        let mut reports: Vec<i64> = if request.report_files {
            self.column(MEMBER_REPORTS, [files])?
        } else {
            Vec::new()
        };
        if let DeleteTarget::Series(batch) = request.target {
            reports.extend(self.column::<i64>(BATCH_REPORTS, [batch.0])?);
        }
        reports.sort_unstable();
        reports.dedup();
        Ok(reports)
    }

    fn remove_target(&self, request: DeleteRequest, now: Timestamp) -> Result<(), TxError> {
        let server = request.server;
        match request.target {
            DeleteTarget::Series(batch) => {
                self.trash_series(batch)?;
                self.run(
                    "UPDATE batch SET deleted_at = ?2, needs_attention = 0, delete_on_server = ?3 WHERE id = ?1",
                    params![batch.0, now.0, server],
                )
            }
            DeleteTarget::Image(batch, file) => self.trash_image((batch, file), server, now),
        }
    }

    fn members(
        &mut self,
        target: DeleteTarget,
        now: Timestamp,
    ) -> Result<(Vec<i64>, String), TxError> {
        let batch = target.batch();
        let (open, label) = self.live_batch(batch)?;
        match target {
            DeleteTarget::Series(_) => {
                if open {
                    BatchRepo::close(self, batch, ClosureReason::Manual, now)?;
                }
                Ok((self.column(FILES, [batch.0])?, label))
            }
            DeleteTarget::Image(_, file) => Ok((vec![file.0], self.image_label(batch, file)?)),
        }
    }

    pub(super) fn delete_history(
        &mut self,
        request: DeleteRequest,
        now: Timestamp,
    ) -> Result<Deletion, TxError> {
        let (files, label) = self.members(request.target, now)?;
        self.stop_work(&files, now)?;
        let list = id_list(&files, |id| id);
        let jobs: Vec<String> = self.column(JOBS, [&list])?;
        let server_images: Vec<String> = self.column(IMAGES, [&list])?;
        self.run(RELEASE_LEDGER, [&list])?;
        let reports = self.affected_reports(request, &list)?;
        let parts = self.hide_reports(&reports, request.report_files, now)?;
        self.remove_target(request, now)?;
        let deletion = Deletion {
            images: u32::try_from(files.len()).unwrap_or(u32::MAX),
            jobs,
            server_images,
            parts,
        };
        self.log_event(&deleted_event(request, &deletion, label, now))?;
        self.record(Change::All);
        Ok(deletion)
    }
}

use mivia_domain::ids::{BatchId, ComputationId, ReportId, ReportPartId, Timestamp};
use mivia_domain::label::MEMBER_LABEL_MAX_CHARS;
use mivia_domain::natural::sort_key;
use mivia_domain::path::WinPath;
use mivia_domain::sanitise::clean_text;
use mivia_domain::transition::IllegalTransition;
use mivia_ports::ipc::change::Change;
use mivia_ports::store::TxError;
use mivia_ports::store::history::{
    DeleteRequest, Deletion, HistoryRepo, MemberCandidate, MoveSet, Moved, NewCustomReport,
    NewEvent, Trigger,
};
use rusqlite::{OptionalExtension, Row, params};

use super::SqlTx;
use super::batches::write_search_doc;
use crate::codec::{COMPUTATION_STATES, computation_state, decode};
use crate::errors::SqlResult;
use crate::sql::history as sql;

const INSERT_EVENT: &str = "INSERT INTO history_event (at, tz, windows_user, account_id, \
     batch_id, report_id, action, trigger, before_json, after_json) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)";

const SAVED_PART: &str = "SELECT r.batch_id, r.id, json_object('part', p.part_no, \
     'parts', p.part_count, 'format', p.format, 'sha256', lower(hex(p.sha256)), 'size', p.size, \
     'appVersion', p.app_version, 'serverVersion', p.server_version, \
     'serverReportId', p.server_report_id) FROM report_part p \
     JOIN report r ON r.id = p.report_id WHERE p.id = ?1";

const CANDIDATES: &str = "SELECT c.id, c.batch_id, c.model_choice_key, s.label, s.arrival_seq, \
     c.state, c.reason, s.state = 'rejected', c.done_at FROM computation c \
     INDEXED BY computation_by_batch JOIN source_file s ON s.id = c.source_file_id \
     WHERE c.batch_id = ?1 AND c.account_id = ?2";

type RawCandidate = (
    i64,
    i64,
    String,
    String,
    i64,
    (String, Option<String>),
    bool,
    Option<i64>,
);

fn raw_candidate(row: &Row<'_>) -> rusqlite::Result<RawCandidate> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        (row.get(5)?, row.get(6)?),
        row.get(7)?,
        row.get(8)?,
    ))
}

fn candidate(raw: RawCandidate) -> Result<MemberCandidate, TxError> {
    let (computation, batch, choice_key, label, arrival, (state, reason), rejected, done) = raw;
    Ok(MemberCandidate {
        computation: ComputationId(computation),
        batch: BatchId(batch),
        choice_key,
        label,
        arrival,
        state: decode(
            "computation",
            &COMPUTATION_STATES,
            computation_state,
            (&state, reason.as_deref()),
        )?,
        file_rejected: rejected,
        done_at: done.map(Timestamp),
    })
}

pub(super) fn json(pairs: &[(&str, String)]) -> String {
    let map: serde_json::Map<String, serde_json::Value> = pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), serde_json::Value::String(value.clone())))
        .collect();
    serde_json::Value::Object(map).to_string()
}

fn series_label(label: &str) -> String {
    let cleaned = clean_text(label);
    cleaned
        .trim()
        .chars()
        .take(MEMBER_LABEL_MAX_CHARS)
        .collect::<String>()
        .trim()
        .to_owned()
}

impl SqlTx<'_> {
    pub(super) fn log_event(&mut self, event: &NewEvent) -> Result<(), TxError> {
        let values = params![
            event.at.0,
            self.actor.tz,
            self.actor.windows_user,
            self.account(),
            event.batch.map(|batch| batch.0),
            event.report.map(|report| report.0),
            event.action,
            event.trigger.code(),
            event.before,
            event.after
        ];
        self.run(INSERT_EVENT, values)?;
        if let Some(batch) = event.batch {
            self.record(Change::BatchItems {
                batch_id: batch.0.to_string(),
            });
        }
        Ok(())
    }

    pub(super) fn log_saved(
        &mut self,
        part: ReportPartId,
        path: &WinPath,
        now: Timestamp,
    ) -> Result<Option<i64>, TxError> {
        let (batch, report, details): (Option<i64>, i64, String) = self
            .tx
            .query_row(SAVED_PART, [part.0], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .tx()?;
        let mut event = NewEvent::new("report_saved", Trigger::Automatic, now);
        event.batch = batch.map(BatchId);
        event.report = Some(ReportId(report));
        let mut after: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&details).unwrap_or_default();
        after.insert("path".to_owned(), path.to_string().into());
        event.after = Some(serde_json::Value::Object(after).to_string());
        self.log_event(&event)?;
        Ok(batch)
    }

    fn rename_series(&mut self, id: BatchId, label: &str, now: Timestamp) -> Result<(), TxError> {
        let label = series_label(label);
        let before: Option<String> = self
            .tx
            .query_row(
                "SELECT label FROM batch WHERE id = ?1 AND account_id = ?2",
                [id.0, self.account()],
                |row| row.get(0),
            )
            .optional()
            .tx()?;
        let before = before.ok_or(TxError::NotFound)?;
        let allowed: bool = self
            .tx
            .query_row(sql::RENAMABLE, [id.0], |row| row.get(0))
            .tx()?;
        if !allowed || label.is_empty() {
            return Err(TxError::Illegal(IllegalTransition("rename")));
        }
        self.run(
            "UPDATE batch SET label = ?2, label_user_set = 1, label_sort = ?3 WHERE id = ?1",
            params![id.0, label, sort_key(&label)],
        )?;
        write_search_doc(self.tx, id.0).tx()?;
        let mut event = NewEvent::new("series_renamed", Trigger::User, now);
        event.batch = Some(id);
        event.before = Some(json(&[("label", before)]));
        event.after = Some(json(&[("label", label)]));
        self.log_event(&event)?;
        self.record(Change::History);
        Ok(())
    }

    fn report_candidates(&self, batches: &[BatchId]) -> Result<Vec<MemberCandidate>, TxError> {
        let mut out = Vec::new();
        for batch in batches {
            let raws = self.rows(CANDIDATES, [batch.0, self.account()], raw_candidate)?;
            for raw in raws {
                out.push(candidate(raw)?);
            }
        }
        Ok(out)
    }
}

impl HistoryRepo for SqlTx<'_> {
    fn log(&mut self, event: &NewEvent) -> Result<(), TxError> {
        self.log_event(event)
    }

    fn rename(&mut self, id: BatchId, label: &str, now: Timestamp) -> Result<(), TxError> {
        self.rename_series(id, label, now)
    }

    fn candidates(&mut self, batches: &[BatchId]) -> Result<Vec<MemberCandidate>, TxError> {
        self.report_candidates(batches)
    }

    fn create_custom(&mut self, report: &NewCustomReport) -> Result<ReportId, TxError> {
        self.insert_custom(report)
    }

    fn update_stale(&mut self, id: ReportId, now: Timestamp) -> Result<ReportId, TxError> {
        self.update_report(id, now)
    }

    fn keep_as_is(&mut self, id: ReportId, now: Timestamp) -> Result<Option<BatchId>, TxError> {
        self.keep_report(id, now)
    }

    fn move_set(&mut self, id: ReportId) -> Result<MoveSet, TxError> {
        self.read_move_set(id)
    }

    fn moved(&mut self, part: ReportPartId, path: &WinPath, how: Moved) -> Result<(), TxError> {
        self.record_move(part, path, how)
    }

    fn delete(&mut self, request: DeleteRequest, now: Timestamp) -> Result<Deletion, TxError> {
        self.delete_history(request, now)
    }
}

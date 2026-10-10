use mivia_domain::ids::{ReportId, ReportPartId, Timestamp};
use mivia_domain::report::{PartEvent, PartState, split_parts};
use mivia_ports::ipc::change::Change;
use mivia_ports::store::{NewReport, ReportRepo, TxError};
use rusqlite::params;

use super::counters::{self, Counts};
use super::{SqlTx, id_list};
use crate::codec::{PART_STATES, decode, part_state};
use crate::errors::SqlResult;
use crate::sql;

const INSERT_REPORT: &str = "INSERT INTO report (account_id, kind, batch_id, model_choice_key, \
     formats, language, state, created_at, app_version) \
     SELECT ?1, 'batch', b.id, ?3, ?4, ?5, 'creating', ?6, ?7 FROM batch b \
     WHERE b.id = ?2 AND b.account_id = ?1 RETURNING id";

const INSERT_MEMBER: &str = "INSERT INTO report_member (report_id, seq, computation_id, label) \
     SELECT ?1, ?2, c.id, ?4 FROM computation c WHERE c.id = ?3 AND c.account_id = ?5";

const INSERT_PART: &str = "INSERT INTO report_part (report_id, part_no, part_count, format, \
     member_from, member_to, state, app_version) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'requested', ?7)";

const PART_ROW: &str = "SELECT r.batch_id, p.state, p.error, r.stale = 1 AND r.replaced_by IS NULL \
     FROM report_part p JOIN report r ON r.id = p.report_id WHERE p.id = ?1 AND r.account_id = ?2";

const UPDATE_PART: &str = "UPDATE report_part SET state = ?2, error = ?3, \
     saved_at = CASE WHEN ?2 = 'saved' THEN ?4 ELSE saved_at END WHERE id = ?1";

type PartRow = (Option<i64>, String, Option<String>, bool);

impl SqlTx<'_> {
    fn insert_members(&self, report_id: i64, report: &NewReport) -> Result<(), TxError> {
        let mut insert = self.tx.prepare_cached(INSERT_MEMBER).tx()?;
        for (seq, (computation, label)) in (1_i64..).zip(&report.members) {
            let inserted = insert
                .execute(params![
                    report_id,
                    seq,
                    computation.0,
                    label,
                    self.account()
                ])
                .tx()?;
            if inserted == 0 {
                return Err(TxError::NotFound);
            }
        }
        Ok(())
    }

    fn insert_parts(&self, report_id: i64, report: &NewReport) -> Result<i64, TxError> {
        let parts = split_parts(report.members.len());
        let count = i64::try_from(parts.len()).map_err(|_| TxError::Conflict)?;
        let mut insert = self.tx.prepare_cached(INSERT_PART).tx()?;
        for format in &report.formats {
            for (number, range) in (1_i64..).zip(&parts) {
                insert
                    .execute(params![
                        report_id,
                        number,
                        count,
                        format,
                        i64::try_from(range.start).map_err(|_| TxError::Conflict)?,
                        i64::try_from(range.end).map_err(|_| TxError::Conflict)?,
                        report.app_version
                    ])
                    .tx()?;
            }
        }
        Ok(count * i64::try_from(report.formats.len()).map_err(|_| TxError::Conflict)?)
    }

    fn reports_changed(
        &mut self,
        batch: Option<i64>,
        change: (Counts, Counts),
        now: Timestamp,
    ) -> Result<(), TxError> {
        let Some(batch_id) = batch else {
            return Ok(());
        };
        counters::shift(self.tx, batch_id, change, now).tx()?;
        self.record(Change::Reports {
            batch_id: batch_id.to_string(),
        });
        Ok(())
    }
}

impl ReportRepo for SqlTx<'_> {
    fn create(&mut self, report: &NewReport) -> Result<ReportId, TxError> {
        let id: i64 = self
            .tx
            .prepare_cached(INSERT_REPORT)
            .and_then(|mut statement| {
                statement.query_row(
                    params![
                        self.account(),
                        report.batch_id.0,
                        report.model_choice_key,
                        report.formats.join(","),
                        report.language,
                        report.created_at.0,
                        report.app_version
                    ],
                    |row| row.get(0),
                )
            })
            .tx()?;
        self.insert_members(id, report)?;
        let parts = self.insert_parts(id, report)?;
        let added = Counts {
            reports_total: parts,
            ..Counts::default()
        };
        self.reports_changed(
            Some(report.batch_id.0),
            (Counts::default(), added),
            report.created_at,
        )?;
        Ok(ReportId(id))
    }

    fn parts(&mut self, id: ReportId) -> Result<Vec<ReportPartId>, TxError> {
        let ids = self.column(
            "SELECT p.id FROM report_part p JOIN report r ON r.id = p.report_id \
             WHERE p.report_id = ?1 AND r.account_id = ?2 ORDER BY p.format, p.part_no",
            [id.0, self.account()],
        )?;
        Ok(ids.into_iter().map(ReportPartId).collect())
    }

    fn apply_part(
        &mut self,
        id: ReportPartId,
        event: PartEvent,
        now: Timestamp,
    ) -> Result<PartState, TxError> {
        let (batch, state, error, in_stale_report): PartRow = self
            .tx
            .query_row(PART_ROW, [id.0, self.account()], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .tx()?;
        let before = decode(
            "report part",
            &PART_STATES,
            part_state,
            (&state, error.as_deref()),
        )?;
        let after = before.apply(event)?;
        let (code, detail) = part_state(after);
        self.tx
            .prepare_cached(UPDATE_PART)
            .and_then(|mut statement| statement.execute(params![id.0, code, detail, now.0]))
            .tx()?;
        let change = (
            counters::part(Some(before), in_stale_report),
            counters::part(Some(after), in_stale_report),
        );
        self.reports_changed(batch, change, now)?;
        Ok(after)
    }

    fn set_stale(&mut self, id: ReportId, stale: bool) -> Result<(), TxError> {
        let (batch, was, open, parts): (Option<i64>, bool, bool, i64) = self
            .tx
            .query_row(
                "SELECT batch_id, stale, replaced_by IS NULL, \
                 (SELECT count(*) FROM report_part WHERE report_id = report.id) \
                 FROM report WHERE id = ?1 AND account_id = ?2",
                [id.0, self.account()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .tx()?;
        self.tx
            .execute(
                "UPDATE report SET stale = ?2 WHERE id = ?1",
                params![id.0, stale],
            )
            .tx()?;
        let counted = |flag: bool| Counts {
            reports_stale: i64::from(flag && open) * parts,
            ..Counts::default()
        };
        self.reports_changed(batch, (counted(was), counted(stale)), Timestamp(0))
    }

    fn due_for_request(&mut self, exclude: &[ReportPartId]) -> Result<Vec<ReportPartId>, TxError> {
        self.parts_due(&sql::part_due_request(), Timestamp(0), exclude)
    }

    fn due_for_poll(
        &mut self,
        now: Timestamp,
        exclude: &[ReportPartId],
    ) -> Result<Vec<ReportPartId>, TxError> {
        self.parts_due(&sql::part_due_poll(), now, exclude)
    }
}

impl SqlTx<'_> {
    fn parts_due(
        &self,
        query: &str,
        now: Timestamp,
        exclude: &[ReportPartId],
    ) -> Result<Vec<ReportPartId>, TxError> {
        let values = params![
            self.account(),
            now.0,
            id_list(exclude, |id| id.0),
            sql::CLAIM_LIMIT
        ];
        let ids = self.column(query, values)?;
        Ok(ids.into_iter().map(ReportPartId).collect())
    }
}

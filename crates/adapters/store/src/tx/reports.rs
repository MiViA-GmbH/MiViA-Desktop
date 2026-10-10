use mivia_domain::ids::{ReportId, ReportPartId, Timestamp};
use mivia_domain::report::{PartEvent, PartState, split_parts};
use mivia_ports::ipc::change::Change;
use mivia_ports::store::{NewReport, TxError};
use rusqlite::params;

use super::SqlTx;
use super::counters::{self, Counts};
use crate::codec::{PART_STATES, decode, part_state};
use crate::errors::SqlResult;

const INSERT_REPORT: &str = "INSERT INTO report (account_id, kind, batch_id, model_choice_key, \
     formats, language, state, created_at, app_version) \
     SELECT ?1, 'batch', b.id, ?3, ?4, ?5, 'creating', ?6, ?7 FROM batch b \
     WHERE b.id = ?2 AND b.account_id = ?1 RETURNING id";

const INSERT_MEMBER: &str = "INSERT INTO report_member (report_id, seq, computation_id, label, \
     omitted) SELECT ?1, ?2, c.id, ?4, ?6 FROM computation c WHERE c.id = ?3 AND c.account_id = ?5";

const INSERT_PART: &str = "INSERT INTO report_part (report_id, part_no, part_count, format, \
     member_from, member_to, state, app_version) \
     SELECT ?1, ?2, ?3, ?4, ?5, ?6, 'requested', app_version FROM report WHERE id = ?1";

const PART_ROW: &str = "SELECT r.batch_id, p.state, p.error, r.stale = 1 AND r.replaced_by IS NULL \
     FROM report_part p JOIN report r ON r.id = p.report_id WHERE p.id = ?1 AND r.account_id = ?2";

const UPDATE_PART: &str = "UPDATE report_part SET state = ?2, error = ?3, \
     saved_at = CASE WHEN ?2 = 'saved' THEN ?4 ELSE saved_at END WHERE id = ?1";

type PartRow = (Option<i64>, String, Option<String>, bool);

pub(super) struct Member<'a> {
    pub(super) computation: i64,
    pub(super) label: &'a str,
    pub(super) omitted: Option<&'a str>,
}

fn index(value: usize) -> Result<i64, TxError> {
    i64::try_from(value).map_err(|_| TxError::Conflict)
}

impl SqlTx<'_> {
    pub(super) fn insert_members(
        &self,
        report_id: i64,
        members: &[Member<'_>],
    ) -> Result<(), TxError> {
        let mut insert = self.tx.prepare_cached(INSERT_MEMBER).tx()?;
        for (seq, member) in (1_i64..).zip(members) {
            let values = params![
                report_id,
                seq,
                member.computation,
                member.label,
                self.account(),
                member.omitted
            ];
            if insert.execute(values).tx()? == 0 {
                return Err(TxError::NotFound);
            }
        }
        Ok(())
    }

    pub(super) fn insert_parts(
        &self,
        report_id: i64,
        included: usize,
        formats: &[&str],
    ) -> Result<i64, TxError> {
        let parts = split_parts(included);
        let count = index(parts.len())?;
        let mut insert = self.tx.prepare_cached(INSERT_PART).tx()?;
        for format in formats {
            for (number, range) in (1_i64..).zip(&parts) {
                let values = params![
                    report_id,
                    number,
                    count,
                    format,
                    index(range.start)?,
                    index(range.end)?
                ];
                insert.execute(values).tx()?;
            }
        }
        Ok(count * index(formats.len())?)
    }

    pub(super) fn reports_changed(
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

    pub(super) fn create_report(&mut self, report: &NewReport) -> Result<ReportId, TxError> {
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
        let members: Vec<Member<'_>> = report
            .members
            .iter()
            .map(|(computation, label)| Member {
                computation: computation.0,
                label,
                omitted: None,
            })
            .collect();
        self.insert_members(id, &members)?;
        let formats: Vec<&str> = report.formats.iter().map(String::as_str).collect();
        let parts = self.insert_parts(id, members.len(), &formats)?;
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

    pub(super) fn report_parts(&self, id: ReportId) -> Result<Vec<ReportPartId>, TxError> {
        let ids = self.column(
            "SELECT p.id FROM report_part p JOIN report r ON r.id = p.report_id \
             WHERE p.report_id = ?1 AND r.account_id = ?2 ORDER BY p.format DESC, p.part_no",
            [id.0, self.account()],
        )?;
        Ok(ids.into_iter().map(ReportPartId).collect())
    }

    pub(super) fn part_event(
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

    pub(super) fn mark_stale(&mut self, id: ReportId, stale: bool) -> Result<(), TxError> {
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
}

use mivia_domain::ids::{BatchId, ReportId, Timestamp};
use mivia_domain::report::{Membership, OmittedReason, membership};
use mivia_domain::transition::IllegalTransition;
use mivia_ports::ipc::change::Change;
use mivia_ports::store::TxError;
use mivia_ports::store::history::{NewCustomReport, NewEvent, Trigger};
use rusqlite::{OptionalExtension, params};

use super::SqlTx;
use super::counters::Counts;
use super::history::json;
use super::reports::Member;
use crate::codec::{COMPUTATION_STATES, computation_state, decode};
use crate::errors::SqlResult;

const INSERT_CUSTOM: &str = "INSERT INTO report (account_id, kind, model_choice_key, \
     custom_name, formats, language, decimal_separator, include_images, timezone, state, \
     created_at, app_version, folder) \
     VALUES (?1, 'custom', ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'creating', ?9, ?10, ?11) RETURNING id";

const COPY_REPORT: &str = "INSERT INTO report (account_id, kind, batch_id, model_choice_key, \
     custom_name, order_by, formats, language, decimal_separator, include_images, timezone, \
     state, replaces, update_no, folder, created_at, app_version) \
     SELECT account_id, kind, batch_id, model_choice_key, custom_name, order_by, formats, \
     language, decimal_separator, include_images, timezone, 'creating', id, update_no + 1, \
     folder, ?2, app_version FROM report WHERE id = ?1 RETURNING id";

const SUCCESSOR: &str = "SELECT c.superseded_by, c.state, c.reason, s.state = 'rejected' \
     FROM computation c JOIN source_file s ON s.id = c.source_file_id WHERE c.id = ?1";

const LOOP_GUARD: usize = 1000;

type StaleHead = (Option<i64>, String, bool, Option<i64>);

fn custom_members(report: &NewCustomReport) -> Vec<Member<'_>> {
    report
        .members
        .iter()
        .map(|member| Member {
            computation: member.computation.0,
            label: &member.label,
            omitted: member.omitted.map(OmittedReason::code),
        })
        .collect()
}

fn created_event(report: &NewCustomReport, id: i64, formats: String) -> NewEvent {
    let mut event = NewEvent::new("custom_report_created", Trigger::User, report.created_at);
    event.report = Some(ReportId(id));
    event.after = Some(json(&[
        ("name", report.name.clone()),
        ("analysis", report.choice_key.clone()),
        ("formats", formats),
        ("language", report.spec.language.code().to_owned()),
        ("images", report.members.len().to_string()),
    ]));
    event
}

fn updated_event(batch: Option<i64>, old: ReportId, new: i64, now: Timestamp) -> NewEvent {
    let mut event = NewEvent::new("report_updated", Trigger::User, now);
    event.batch = batch.map(BatchId);
    event.report = Some(ReportId(new));
    event.before = Some(json(&[("report", old.0.to_string())]));
    event.after = Some(json(&[("report", new.to_string())]));
    event
}

impl SqlTx<'_> {
    fn add_parts(
        &self,
        report: i64,
        members: &[Member<'_>],
        formats: &str,
    ) -> Result<i64, TxError> {
        self.insert_members(report, members)?;
        let included = members.iter().filter(|m| m.omitted.is_none()).count();
        let formats: Vec<&str> = formats.split(',').filter(|f| !f.is_empty()).collect();
        if included == 0 {
            self.run(
                "UPDATE report SET state = 'nothing' WHERE id = ?1",
                [report],
            )?;
            return Ok(0);
        }
        self.insert_parts(report, included, &formats)
    }

    fn insert_custom_row(&self, report: &NewCustomReport, formats: &str) -> Result<i64, TxError> {
        let separator = if report.spec.decimal_comma { "," } else { "." };
        let values = params![
            self.account(),
            report.choice_key,
            report.name,
            formats,
            report.spec.language.code(),
            separator,
            report.spec.include_images,
            report.spec.timezone,
            report.created_at.0,
            report.app_version,
            report.folder.to_string()
        ];
        self.tx
            .prepare_cached(INSERT_CUSTOM)
            .and_then(|mut statement| statement.query_row(values, |row| row.get(0)))
            .tx()
    }

    pub(super) fn insert_custom(&mut self, report: &NewCustomReport) -> Result<ReportId, TxError> {
        let formats: Vec<&str> = report.spec.formats.iter().map(|f| f.code()).collect();
        let formats = formats.join(",");
        let id = self.insert_custom_row(report, &formats)?;
        self.add_parts(id, &custom_members(report), &formats)?;
        self.log_event(&created_event(report, id, formats))?;
        self.record(Change::History);
        Ok(ReportId(id))
    }

    fn latest(&self, computation: i64) -> Result<(i64, Membership), TxError> {
        let mut current = computation;
        for _ in 0..LOOP_GUARD {
            let (next, state, reason, rejected): (Option<i64>, String, Option<String>, bool) = self
                .tx
                .query_row(SUCCESSOR, [current], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                })
                .tx()?;
            let Some(next) = next else {
                let stored = (state.as_str(), reason.as_deref());
                let state = decode(
                    "computation",
                    &COMPUTATION_STATES,
                    computation_state,
                    stored,
                )?;
                if !state.is_terminal() {
                    return Err(TxError::Conflict);
                }
                return Ok((current, membership(state, rejected)));
            };
            current = next;
        }
        Err(TxError::Corrupt("supersede loop".to_owned()))
    }

    fn stale_head(&self, id: ReportId) -> Result<(Option<i64>, String), TxError> {
        let head: Option<StaleHead> = self
            .tx
            .query_row(
                "SELECT batch_id, formats, stale, replaced_by FROM report WHERE id = ?1 AND account_id = ?2",
                [id.0, self.account()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .tx()?;
        match head.ok_or(TxError::NotFound)? {
            (batch, formats, true, None) => Ok((batch, formats)),
            _ => Err(TxError::Illegal(IllegalTransition("update"))),
        }
    }

    fn successors(&self, id: ReportId) -> Result<Vec<(i64, String, Membership)>, TxError> {
        let old: Vec<(i64, String)> = self.rows(
            "SELECT computation_id, label FROM report_member WHERE report_id = ?1 ORDER BY seq",
            [id.0],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        old.into_iter()
            .map(|(computation, label)| {
                let (computation, member) = self.latest(computation)?;
                Ok((computation, label, member))
            })
            .collect()
    }

    pub(super) fn update_report(
        &mut self,
        id: ReportId,
        now: Timestamp,
    ) -> Result<ReportId, TxError> {
        let (batch, formats) = self.stale_head(id)?;
        let latest = self.successors(id)?;
        let new_id: i64 = self
            .tx
            .query_row(COPY_REPORT, params![id.0, now.0], |row| row.get(0))
            .tx()?;
        let parts = self.add_parts(new_id, &ordered_members(&latest), &formats)?;
        let old_parts: Vec<i64> = self.column(
            "SELECT count(*) FROM report_part WHERE report_id = ?1",
            [id.0],
        )?;
        self.run(
            "UPDATE report SET replaced_by = ?2 WHERE id = ?1",
            [id.0, new_id],
        )?;
        let before = Counts {
            reports_stale: old_parts.into_iter().sum(),
            ..Counts::default()
        };
        let after = Counts {
            reports_total: parts,
            ..Counts::default()
        };
        self.reports_changed(batch, (before, after), now)?;
        self.settle(batch, now, false)?;
        self.log_event(&updated_event(batch, id, new_id, now))?;
        self.record(Change::History);
        Ok(ReportId(new_id))
    }
}

fn ordered_members(latest: &[(i64, String, Membership)]) -> Vec<Member<'_>> {
    let included = latest
        .iter()
        .filter(|(_, _, m)| *m == Membership::Included)
        .map(|(c, l, _)| Member {
            computation: *c,
            label: l,
            omitted: None,
        });
    let omitted = latest.iter().filter_map(|(c, l, m)| match m {
        Membership::Omitted(reason) => Some(Member {
            computation: *c,
            label: l,
            omitted: Some(reason.code()),
        }),
        Membership::Included | Membership::Excluded => None,
    });
    included.chain(omitted).collect()
}

use std::cmp::Ordering;

use mivia_domain::ids::{ReportId, Timestamp};
use mivia_domain::natural;
use mivia_domain::report::{Membership, membership};
use mivia_ports::store::TxError;
use mivia_ports::store::reports::{ReportSpec, WaitingReport};
use rusqlite::{Row, params};

use super::SqlTx;
use super::counters::Counts;
use super::reports::Member;
use crate::codec::{COMPUTATION_STATES, computation_state, decode};
use crate::errors::SqlResult;
use crate::sql::reports as sql;

struct Candidate {
    computation: i64,
    membership: Membership,
    label: String,
    arrival: i64,
}

type RawCandidate = (i64, String, Option<String>, String, i64, bool);

fn raw_candidate(row: &Row<'_>) -> rusqlite::Result<RawCandidate> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
    ))
}

fn candidate(raw: RawCandidate) -> Result<Candidate, TxError> {
    let (computation, state, reason, label, arrival, rejected) = raw;
    let state = decode(
        "computation",
        &COMPUTATION_STATES,
        computation_state,
        (&state, reason.as_deref()),
    )?;
    Ok(Candidate {
        computation,
        membership: membership(state, rejected),
        label,
        arrival,
    })
}

fn order(a: &Candidate, b: &Candidate) -> Ordering {
    natural::compare(&a.label, &b.label).then(a.arrival.cmp(&b.arrival))
}

fn members(candidates: &[Candidate]) -> Vec<Member<'_>> {
    let included = candidates
        .iter()
        .filter(|c| c.membership == Membership::Included)
        .map(|c| (c, None));
    let omitted = candidates.iter().filter_map(|c| match c.membership {
        Membership::Omitted(reason) => Some((c, Some(reason.code()))),
        Membership::Included | Membership::Excluded => None,
    });
    included
        .chain(omitted)
        .map(|(c, omitted)| Member {
            computation: c.computation,
            label: &c.label,
            omitted,
        })
        .collect()
}

impl SqlTx<'_> {
    pub(super) fn waiting_reports(&self) -> Result<Vec<WaitingReport>, TxError> {
        self.rows(sql::WAITING, [self.account()], |row| {
            Ok(WaitingReport {
                id: ReportId(row.get(0)?),
                settings_json: row.get(1)?,
                pending: row.get(2)?,
            })
        })
    }

    pub(super) fn insert_waiting(&self, batch: i64, now: Timestamp) -> Result<(), TxError> {
        self.run(sql::INSERT_WAITING, params![batch, now.0])
    }

    fn candidates(&self, batch: i64, choice: &str) -> Result<Vec<Candidate>, TxError> {
        let raws = self.rows(
            sql::BUILD_MEMBERS,
            params![batch, choice, self.account()],
            raw_candidate,
        )?;
        let mut candidates = raws
            .into_iter()
            .map(candidate)
            .collect::<Result<Vec<_>, _>>()?;
        candidates.sort_by(order);
        Ok(candidates)
    }

    fn settle_report(&self, id: ReportId, spec: &ReportSpec, parts: i64) -> Result<(), TxError> {
        let formats: Vec<&str> = spec.formats.iter().map(|f| f.code()).collect();
        let state = if parts == 0 { "nothing" } else { "creating" };
        let separator = if spec.decimal_comma { "," } else { "." };
        self.run(
            "UPDATE report SET formats = ?2, language = ?3, decimal_separator = ?4, \
             include_images = ?5, timezone = ?6, state = ?7 WHERE id = ?1",
            params![
                id.0,
                formats.join(","),
                spec.language.code(),
                separator,
                spec.include_images,
                spec.timezone,
                state
            ],
        )
    }

    pub(super) fn build_report(
        &mut self,
        id: ReportId,
        spec: &ReportSpec,
        now: Timestamp,
    ) -> Result<u32, TxError> {
        let (batch, choice): (i64, String) = self
            .tx
            .query_row(
                "SELECT batch_id, model_choice_key FROM report \
                 WHERE id = ?1 AND account_id = ?2 AND state = 'waiting'",
                [id.0, self.account()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .tx()?;
        let candidates = self.candidates(batch, &choice)?;
        let members = members(&candidates);
        let count = members.iter().filter(|m| m.omitted.is_none()).count();
        self.insert_members(id.0, &members)?;
        let formats: Vec<&str> = spec.formats.iter().map(|f| f.code()).collect();
        let parts = if count == 0 {
            0
        } else {
            self.insert_parts(id.0, count, &formats)?
        };
        self.settle_report(id, spec, parts)?;
        let added = Counts {
            reports_total: parts,
            ..Counts::default()
        };
        self.reports_changed(Some(batch), (Counts::default(), added), now)?;
        Ok(u32::try_from(parts).unwrap_or(u32::MAX))
    }
}

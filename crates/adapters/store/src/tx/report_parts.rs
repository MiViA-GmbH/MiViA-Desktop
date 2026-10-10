use mivia_domain::ids::{ReportPartId, Timestamp};
use mivia_domain::naming::Language;
use mivia_domain::output::Format;
use mivia_domain::report::{OmittedReason, PartEvent, PartState, halve};
use mivia_ports::store::TxError;
use mivia_ports::store::reports::{PollRow, RequestRow, Rerequest};
use rusqlite::{OptionalExtension, Row, params};

use super::SqlTx;
use super::counters::Counts;
use crate::errors::SqlResult;
use crate::sql::reports as sql;

pub(super) struct PartHead {
    pub(super) report: i64,
    format: String,
    number: i64,
    from: i64,
    to: i64,
    split: bool,
    rerequests: u32,
}

fn corrupt(what: &str) -> TxError {
    TxError::Corrupt(format!("report part {what}"))
}

pub(super) fn format_of(code: &str) -> Result<Format, TxError> {
    Format::from_code(code).ok_or_else(|| corrupt(code))
}

pub(super) fn language_of(code: &str) -> Result<Language, TxError> {
    Language::from_code(code).ok_or_else(|| corrupt(code))
}

fn count(value: i64) -> u32 {
    u32::try_from(value).unwrap_or_default()
}

fn narrow(value: i64) -> usize {
    usize::try_from(value).unwrap_or_default()
}

fn wide(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

type PartMember = (i64, Option<String>, String);
type RawPoll = (i64, String, String, i64, i64, bool);

type RawRequest = (
    (i64, String, i64, i64, String, bool),
    (String, String, i64, i64, i64, i64),
);

fn raw_request(row: &Row<'_>) -> rusqlite::Result<RawRequest> {
    Ok((
        (
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
        ),
        (
            row.get(6)?,
            row.get(7)?,
            row.get(8)?,
            row.get(9)?,
            row.get(10)?,
            row.get(11)?,
        ),
    ))
}

impl SqlTx<'_> {
    pub(super) fn part_head(&self, id: ReportPartId) -> Result<PartHead, TxError> {
        self.tx
            .query_row(sql::PART_HEAD, [id.0, self.account()], |row| {
                Ok(PartHead {
                    report: row.get(0)?,
                    format: row.get(1)?,
                    number: row.get(2)?,
                    from: row.get(3)?,
                    to: row.get(4)?,
                    split: row.get(5)?,
                    rerequests: row.get(6)?,
                })
            })
            .tx()
    }

    pub(super) fn report_in_flight(&self) -> Result<bool, TxError> {
        let found: Vec<i64> = self.column(sql::IN_FLIGHT, [])?;
        Ok(!found.is_empty())
    }

    pub(super) fn part_members(
        &self,
        report: i64,
        range: (i64, i64),
    ) -> Result<Vec<PartMember>, TxError> {
        self.rows(
            sql::PART_MEMBERS,
            params![report, range.0, range.1],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
    }

    pub(super) fn first_request(&self, now: Timestamp) -> Result<Option<RequestRow>, TxError> {
        let raw = self
            .tx
            .prepare_cached(&sql::next_request())
            .and_then(|mut statement| {
                statement
                    .query_row(params![self.account(), now.0], raw_request)
                    .optional()
            })
            .tx()?;
        let Some(((id, format, number, total, language, images), rest)) = raw else {
            return Ok(None);
        };
        let (separator, timezone, from, to, report, attempts) = rest;
        let members = self
            .part_members(report, (from, to))?
            .into_iter()
            .map(|(_, job, label)| (job.unwrap_or_default(), label))
            .collect();
        let omitted = if number == 1 {
            self.omitted(report)?
        } else {
            Vec::new()
        };
        Ok(Some(RequestRow {
            id: ReportPartId(id),
            format: format_of(&format)?,
            part: (count(number), count(total)),
            language: language_of(&language)?,
            include_images: images,
            decimal_comma: separator == ",",
            timezone,
            members,
            omitted,
            attempts: count(attempts),
        }))
    }

    fn omitted(&self, report: i64) -> Result<Vec<(String, OmittedReason)>, TxError> {
        let rows: Vec<(String, String)> =
            self.rows(sql::OMITTED, [report], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.into_iter()
            .map(|(label, code)| {
                OmittedReason::from_code(&code)
                    .map(|reason| (label, reason))
                    .ok_or_else(|| corrupt(&code))
            })
            .collect()
    }

    pub(super) fn polls_due(&self, now: Timestamp) -> Result<Vec<PollRow>, TxError> {
        let raws: Vec<RawPoll> =
            self.rows(&sql::poll_due(), params![self.account(), now.0], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })?;
        raws.into_iter()
            .map(|(id, server_id, format, requested_at, rerequests, split)| {
                Ok(PollRow {
                    id: ReportPartId(id),
                    server_id,
                    format: format_of(&format)?,
                    requested_at: Timestamp(requested_at),
                    rerequests: count(rerequests),
                    split,
                })
            })
            .collect()
    }

    pub(super) fn first_poll(&self) -> Result<Option<Timestamp>, TxError> {
        let next: Vec<Option<i64>> = self.column(&sql::next_poll(), [self.account()])?;
        Ok(next.into_iter().flatten().next().map(Timestamp))
    }

    pub(super) fn part_update(
        &self,
        id: ReportPartId,
        set: &str,
        value: impl rusqlite::ToSql,
    ) -> Result<(), TxError> {
        self.run(
            &format!("UPDATE report_part SET {set} WHERE id = ?1"),
            params![id.0, value],
        )
    }

    pub(super) fn ask_again(
        &mut self,
        id: ReportPartId,
        why: Rerequest,
        now: Timestamp,
    ) -> Result<PartState, TxError> {
        let head = self.part_head(id)?;
        let event = match why {
            Rerequest::Expired => PartEvent::Expired,
            Rerequest::Cancelled => PartEvent::Cancelled {
                rerequests: head.rerequests,
            },
            Rerequest::TryAgain => PartEvent::TryAgain,
        };
        let after = self.part_event(id, event, now)?;
        let rerequests = match why {
            Rerequest::Expired => i64::from(head.rerequests),
            Rerequest::Cancelled => i64::from(head.rerequests) + 1,
            Rerequest::TryAgain => 0,
        };
        self.run(
            "UPDATE report_part SET next_attempt_at = 0, rerequests = ?2, \
             server_report_id = CASE WHEN staging_path IS NULL THEN NULL \
             ELSE server_report_id END WHERE id = ?1",
            params![id.0, rerequests],
        )?;
        Ok(after)
    }

    pub(super) fn split_part(
        &mut self,
        id: ReportPartId,
        now: Timestamp,
    ) -> Result<PartState, TxError> {
        let head = self.part_head(id)?;
        let halves = halve(&(narrow(head.from)..narrow(head.to))).filter(|_| !head.split);
        let Some((first, second)) = halves else {
            let event = PartEvent::TooLarge {
                already_split: true,
            };
            return self.part_event(id, event, now);
        };
        self.renumber_after(&head)?;
        self.run(
            "UPDATE report_part SET member_to = ?2, split = 1, server_report_id = NULL, \
             next_attempt_at = 0 WHERE id = ?1",
            params![id.0, wide(first.end)],
        )?;
        self.run(
            "INSERT INTO report_part (report_id, part_no, part_count, format, member_from, \
             member_to, state, split, app_version) SELECT report_id, part_no + 1, part_count, \
             format, ?2, ?3, 'requested', 1, app_version FROM report_part WHERE id = ?1",
            params![id.0, wide(second.start), wide(second.end)],
        )?;
        let batch: Option<i64> = self
            .column("SELECT batch_id FROM report WHERE id = ?1", [head.report])?
            .into_iter()
            .next()
            .flatten();
        let added = Counts {
            reports_total: 1,
            ..Counts::default()
        };
        self.reports_changed(batch, (Counts::default(), added), now)?;
        let event = PartEvent::TooLarge {
            already_split: false,
        };
        self.part_event(id, event, now)
    }

    fn renumber_after(&self, head: &PartHead) -> Result<(), TxError> {
        self.run(
            "UPDATE report_part SET part_no = -(part_no + 1) \
             WHERE report_id = ?1 AND format = ?2 AND part_no > ?3",
            params![head.report, head.format, head.number],
        )?;
        self.run(
            "UPDATE report_part SET part_no = -part_no \
             WHERE report_id = ?1 AND format = ?2 AND part_no < 0",
            params![head.report, head.format],
        )?;
        self.run(
            "UPDATE report_part SET part_count = part_count + 1 \
             WHERE report_id = ?1 AND format = ?2",
            params![head.report, head.format],
        )
    }

    pub(super) fn lose_members(
        &self,
        id: ReportPartId,
        job_ids: &[String],
    ) -> Result<u32, TxError> {
        let head = self.part_head(id)?;
        let members = self.part_members(head.report, (head.from, head.to))?;
        let mut left = 0;
        for (seq, job, _) in members {
            if job.is_some_and(|job| job_ids.contains(&job)) {
                self.run(
                    "UPDATE report_member SET omitted = 'LOST' WHERE report_id = ?1 AND seq = ?2",
                    params![head.report, seq],
                )?;
            } else {
                left += 1;
            }
        }
        Ok(left)
    }
}

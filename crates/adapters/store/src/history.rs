use std::collections::BTreeSet;

use mivia_domain::history::{SearchTerm, fts_phrase, search_term};
use mivia_ports::ipc::history::{HistoryCursor, HistoryFilter, HistoryPage, HistoryStatus};
use mivia_ports::store::AccountScope;
use mivia_ports::time::parse_iso_utc;
use rusqlite::types::Value;
use rusqlite::{OptionalExtension, Row, Transaction};

use crate::history_rows::{by_id, head, report_id_matches, row};
use crate::sql::PAGE_SIZE;
use crate::sql::history::{self as sql, Plan, is_label, window};

struct Candidate {
    id: i64,
    folder: Option<i64>,
    one_off: bool,
    problems: (i64, i64, i64),
    active: i64,
    sort_at: i64,
    account: i64,
    key: Value,
    matched: bool,
}

fn candidate(row: &Row<'_>) -> rusqlite::Result<Candidate> {
    Ok(Candidate {
        id: row.get(0)?,
        folder: row.get(1)?,
        one_off: row.get(2)?,
        problems: (row.get(3)?, row.get(4)?, row.get(5)?),
        active: row.get(6)?,
        sort_at: row.get(7)?,
        account: row.get(8)?,
        key: row.get(9)?,
        matched: row.get::<_, bool>(10)? && row.get::<_, bool>(11)?,
    })
}

pub(crate) fn status_ok(
    status: Option<HistoryStatus>,
    problems: (i64, i64, i64),
    active: i64,
) -> bool {
    match status {
        None => true,
        Some(HistoryStatus::Problems) => problems.0 > 0 || problems.1 > 0,
        Some(HistoryStatus::OutOfDate) => problems.2 > 0,
        Some(HistoryStatus::Active) => active > 0,
    }
}

fn accepts(plan: &Plan<'_>, row: &Candidate) -> bool {
    let checks = [
        row.account == plan.account,
        plan.folder.is_none_or(|folder| row.folder == Some(folder)),
        !plan.filter.manual || row.one_off,
        row.matched,
        status_ok(plan.filter.status, row.problems, row.active),
        plan.range
            .is_none_or(|(from, to)| (from..=to).contains(&row.sort_at)),
    ];
    checks.into_iter().all(|check| check)
}

fn key_text(key: &Value) -> String {
    match key {
        Value::Blob(bytes) => bytes
            .iter()
            .flat_map(|byte| [byte >> 4, byte & 0xF])
            .filter_map(|nibble| char::from_digit(u32::from(nibble), 16))
            .collect(),
        Value::Integer(number) => number.to_string(),
        Value::Null | Value::Real(_) | Value::Text(_) => String::new(),
    }
}

fn key_value(text: &str, label: bool) -> Option<Value> {
    if !label {
        return text.parse().ok().map(Value::Integer);
    }
    let bytes = (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(text.get(at..at + 2)?, 16).ok())
        .collect::<Option<Vec<u8>>>()?;
    Some(Value::Blob(bytes))
}

fn range(filter: &HistoryFilter) -> Option<(i64, i64)> {
    let parse = |text: &Option<String>| text.as_deref().and_then(parse_iso_utc).map(|at| at.0);
    match (parse(&filter.from), parse(&filter.to)) {
        (None, None) => None,
        (from, to) => Some((from.unwrap_or(i64::MIN), to.unwrap_or(i64::MAX - 1))),
    }
}

fn cursor(row: &Candidate) -> HistoryCursor {
    HistoryCursor {
        key: key_text(&row.key),
        id: row.id.to_string(),
    }
}

fn body(
    tx: &Transaction<'_>,
    plan: &mut Plan<'_>,
    after: Option<&HistoryCursor>,
) -> rusqlite::Result<(Vec<i64>, Option<HistoryCursor>)> {
    let label = is_label(sql::driver(plan));
    plan.after =
        after.and_then(|after| Some((key_value(&after.key, label)?, after.id.parse().ok()?)));
    let window = window(plan);
    let mut statement = tx.prepare_cached(&window.sql)?;
    let binds: Vec<(&str, &dyn rusqlite::ToSql)> = window
        .binds
        .iter()
        .map(|(name, value)| (*name, value.as_ref()))
        .collect();
    let rows = statement
        .query_map(binds.as_slice(), candidate)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let page = usize::try_from(PAGE_SIZE).unwrap_or(usize::MAX);
    let mut seen = BTreeSet::new();
    let kept: Vec<&Candidate> = rows
        .iter()
        .filter(|row| accepts(plan, row) && seen.insert(row.id))
        .take(page + 1)
        .collect();
    let full = i64::try_from(rows.len()).unwrap_or(i64::MAX) >= window.limit;
    let next = match (kept.get(page), kept.get(page.saturating_sub(1))) {
        (Some(_), Some(last)) => Some(cursor(last)),
        _ if full => rows.last().map(cursor),
        _ => None,
    };
    Ok((kept.iter().take(page).map(|row| row.id).collect(), next))
}

fn plan<'a>(scope: AccountScope, filter: &'a HistoryFilter, term: Option<&SearchTerm>) -> Plan<'a> {
    Plan {
        account: scope.0.0,
        filter,
        folder: filter.folder.as_deref().and_then(|id| id.parse().ok()),
        range: range(filter),
        fts: term.map(fts_phrase),
        after: None,
    }
}

pub(crate) fn filtered_ids(
    tx: &Transaction<'_>,
    scope: AccountScope,
    filter: &HistoryFilter,
) -> rusqlite::Result<Vec<i64>> {
    let term = filter.q.as_deref().and_then(search_term);
    let mut plan = plan(scope, filter, term.as_ref());
    let mut ids: Vec<i64> = head(tx, &plan, term.as_ref())?
        .iter()
        .filter_map(|row| row.summary.id.parse().ok())
        .collect();
    let mut after = None;
    loop {
        let (page, next) = body(tx, &mut plan, after.as_ref())?;
        ids.extend(page);
        match next {
            Some(next) => after = Some(next),
            None => return Ok(ids),
        }
    }
}

pub(crate) fn list(
    tx: &Transaction<'_>,
    scope: AccountScope,
    filter: &HistoryFilter,
    after: Option<&HistoryCursor>,
) -> rusqlite::Result<HistoryPage> {
    let term = filter.q.as_deref().and_then(search_term);
    let mut plan = plan(scope, filter, term.as_ref());
    let (ids, next) = body(tx, &mut plan, after)?;
    let first = after.is_none();
    let mut ordered = if first {
        report_id_matches(tx, scope, term.as_ref())?
    } else {
        Vec::new()
    };
    let mut seen: BTreeSet<i64> = ordered.iter().copied().collect();
    ordered.extend(ids.into_iter().filter(|id| seen.insert(*id)));
    let mut rows = Vec::with_capacity(ordered.len());
    for id in ordered {
        if let Some(summary) = by_id(tx, scope, id)? {
            rows.push(row(tx, summary, term.as_ref())?);
        }
    }
    Ok(HistoryPage {
        revision: String::new(),
        head: if first {
            head(tx, &plan, term.as_ref())?
        } else {
            Vec::new()
        },
        rows,
        next,
    })
}

pub(crate) fn renamable(tx: &Transaction<'_>, batch: i64) -> rusqlite::Result<bool> {
    Ok(tx
        .prepare_cached(sql::RENAMABLE)?
        .query_row([batch], |row| row.get(0))
        .optional()?
        .unwrap_or(false))
}

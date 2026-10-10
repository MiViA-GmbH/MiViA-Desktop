use std::collections::BTreeMap;

use mivia_domain::history::{SCAN_BUDGET, matches, search_term};
use mivia_domain::ids::Timestamp;
use mivia_ports::ipc::history::{
    CustomReportRow, CustomReportsPage, HistoryEventView, HistoryEventsPage,
};
use mivia_ports::ipc::screens::AnalysisView;
use mivia_ports::store::AccountScope;
use mivia_ports::time::iso_utc;
use rusqlite::{Row, Transaction, params};

use crate::screens;
use crate::sql::PAGE_SIZE;
use crate::sql::history as sql;

fn split_cursor(after: Option<&str>) -> (i64, i64) {
    after
        .and_then(|text| text.split_once(':'))
        .and_then(|(key, id)| Some((key.parse().ok()?, id.parse().ok()?)))
        .unwrap_or((i64::MAX, i64::MAX))
}

struct Head {
    id: i64,
    name: String,
    created_at: i64,
    state: String,
    stale: bool,
    choice: String,
    images: u32,
    batch: Option<i64>,
    problem: bool,
    saved: Option<i64>,
}

fn head(row: &Row<'_>) -> rusqlite::Result<Head> {
    Ok(Head {
        id: row.get(0)?,
        name: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
        created_at: row.get(2)?,
        state: row.get(3)?,
        stale: row.get(4)?,
        choice: row.get(5)?,
        images: row.get(6)?,
        batch: row.get(7)?,
        problem: row.get(8)?,
        saved: row.get(9)?,
    })
}

fn analysis(tx: &Transaction<'_>, head: &Head) -> rusqlite::Result<AnalysisView> {
    let found = match head.batch {
        Some(batch) => screens::analyses(tx, batch)?
            .into_iter()
            .find(|analysis| analysis.key == head.choice),
        None => None,
    };
    Ok(found.unwrap_or_else(|| AnalysisView {
        key: head.choice.clone(),
        model_id: head.choice.split(':').next().unwrap_or_default().to_owned(),
        names: BTreeMap::default(),
    }))
}

fn custom_row(tx: &Transaction<'_>, id: i64) -> rusqlite::Result<CustomReportRow> {
    let head = tx.prepare_cached(sql::CUSTOM_ROW)?.query_row([id], head)?;
    Ok(CustomReportRow {
        analysis: analysis(tx, &head)?,
        id: head.id.to_string(),
        name: head.name,
        created_at: iso_utc(Timestamp(head.created_at)),
        images: head.images,
        state: head.state,
        stale: head.stale,
        problem: head.problem,
        saved_part: head.saved.map(|part| part.to_string()),
    })
}

type Windowed = (i64, i64, Option<String>);

fn custom_window(
    tx: &Transaction<'_>,
    scope: AccountScope,
    (key, id): (i64, i64),
    limit: i64,
) -> rusqlite::Result<Vec<Windowed>> {
    tx.prepare_cached(sql::CUSTOM_WINDOW)?
        .query_map(params![scope.0.0, key, id, limit], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect()
}

pub(crate) fn custom_reports(
    tx: &Transaction<'_>,
    scope: AccountScope,
    q: Option<&str>,
    after: Option<&str>,
) -> rusqlite::Result<CustomReportsPage> {
    let term = q.and_then(search_term);
    let limit = if term.is_some() {
        SCAN_BUDGET
    } else {
        PAGE_SIZE + 1
    };
    let window = custom_window(tx, scope, split_cursor(after), limit)?;
    let page = usize::try_from(PAGE_SIZE).unwrap_or(usize::MAX);
    let kept: Vec<&Windowed> = window
        .iter()
        .filter(|(_, _, name)| {
            term.as_ref()
                .is_none_or(|term| matches(term, name.as_deref().unwrap_or_default()))
        })
        .take(page + 1)
        .collect();
    let full = i64::try_from(window.len()).unwrap_or(i64::MAX) >= limit;
    let last = match (kept.get(page), kept.get(page.saturating_sub(1))) {
        (Some(_), Some(last)) => Some(*last),
        _ if full => window.last(),
        _ => None,
    };
    let rows = kept
        .iter()
        .take(page)
        .map(|(id, _, _)| custom_row(tx, *id))
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(CustomReportsPage {
        revision: String::new(),
        rows,
        next: last.map(|(id, at, _)| format!("{at}:{id}")),
    })
}

fn event(row: &Row<'_>) -> rusqlite::Result<HistoryEventView> {
    Ok(HistoryEventView {
        id: row.get::<_, i64>(0)?.to_string(),
        at: iso_utc(Timestamp(row.get(1)?)),
        tz: row.get(2)?,
        windows_user: row.get(3)?,
        action: row.get(4)?,
        trigger: row.get(5)?,
        before: row.get(6)?,
        after: row.get(7)?,
    })
}

pub(crate) fn batch_events(
    tx: &Transaction<'_>,
    scope: AccountScope,
    batch: i64,
    after: Option<&str>,
) -> rusqlite::Result<HistoryEventsPage> {
    let before = after.and_then(|id| id.parse().ok()).unwrap_or(i64::MAX);
    let mut events = tx
        .prepare_cached(sql::BATCH_EVENTS)?
        .query_map(params![scope.0.0, batch, before, PAGE_SIZE + 1], event)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let page = usize::try_from(PAGE_SIZE).unwrap_or(usize::MAX);
    let next = (events.len() > page)
        .then(|| events.truncate(page))
        .and_then(|()| events.last())
        .map(|last| last.id.clone());
    Ok(HistoryEventsPage {
        revision: String::new(),
        events,
        next,
    })
}

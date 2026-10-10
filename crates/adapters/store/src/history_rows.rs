use mivia_domain::history::{SearchTerm, matches};
use mivia_domain::path::WinPath;
use mivia_ports::ipc::history::{HistoryMatch, HistoryRow, MatchKind};
use mivia_ports::ipc::views::{BatchSummary, OriginView};
use mivia_ports::store::AccountScope;
use mivia_ports::time::parse_iso_utc;
use rusqlite::Transaction;

use crate::history::status_ok;
use crate::queries::summaries;
use crate::screens;
use crate::sql::history::{self as sql, Plan};
use crate::sql::{self as base, screens as screen_sql};

fn head_accepts(
    plan: &Plan<'_>,
    row: &HistoryRow,
    term: Option<&SearchTerm>,
    labels: &[String],
) -> bool {
    let summary = &row.summary;
    let counts = &summary.counts;
    let folder = match &summary.origin {
        OriginView::Folder { folder_id } => folder_id.parse().ok(),
        OriginView::OneOff => None,
    };
    let problems = (
        i64::from(counts.problems),
        i64::from(counts.reports_problem),
        i64::from(counts.reports_stale),
    );
    let opened = summary_time(&summary.opened_at);
    let checks = [
        plan.folder.is_none_or(|wanted| folder == Some(wanted)),
        !plan.filter.manual || folder.is_none(),
        plan.filter
            .analysis
            .as_ref()
            .is_none_or(|model| row.analyses.iter().any(|a| &a.model_id == model)),
        status_ok(plan.filter.status, problems, 1),
        plan.range.is_none_or(|(_, to)| opened <= to),
        term.is_none_or(|term| {
            matches(term, &summary.label) || labels.iter().any(|l| matches(term, l))
        }),
    ];
    checks.into_iter().all(|check| check)
}

fn summary_time(iso: &str) -> i64 {
    parse_iso_utc(iso).map_or(0, |at| at.0)
}

pub(crate) fn row(
    tx: &Transaction<'_>,
    summary: BatchSummary,
    term: Option<&SearchTerm>,
) -> rusqlite::Result<HistoryRow> {
    let id: i64 = summary.id.parse().unwrap_or_default();
    let saved: Option<i64> = tx
        .prepare_cached(screen_sql::LATEST_SAVED_PART)?
        .query_row([id], |row| row.get(0))?;
    Ok(HistoryRow {
        analyses: screens::analyses(tx, id)?,
        matched: term
            .map(|term| matched(tx, id, &summary.label, term))
            .transpose()?
            .flatten(),
        saved_part: saved.map(|part| part.to_string()),
        summary,
    })
}

fn texts(tx: &Transaction<'_>, query: &str, batch: i64) -> rusqlite::Result<Vec<String>> {
    tx.prepare_cached(query)?
        .query_map([batch], |row| row.get(0))?
        .collect()
}

fn matched(
    tx: &Transaction<'_>,
    batch: i64,
    label: &str,
    term: &SearchTerm,
) -> rusqlite::Result<Option<HistoryMatch>> {
    if matches(term, label) {
        return Ok(None);
    }
    let image = texts(tx, sql::MEMBER_LABELS, batch)?
        .into_iter()
        .find(|name| matches(term, name))
        .map(|name| HistoryMatch {
            kind: MatchKind::Image,
            name,
        });
    if image.is_some() {
        return Ok(image);
    }
    Ok(texts(tx, sql::SAVED_FILES, batch)?
        .into_iter()
        .filter_map(|path| WinPath::parse(&path).ok()?.file_name().map(str::to_owned))
        .find(|name| matches(term, name))
        .map(|name| HistoryMatch {
            kind: MatchKind::Report,
            name,
        }))
}

pub(crate) fn by_id(
    tx: &Transaction<'_>,
    scope: AccountScope,
    id: i64,
) -> rusqlite::Result<Option<BatchSummary>> {
    Ok(summaries(tx, &base::batch_by_id(), [scope.0.0, id])?
        .into_iter()
        .next()
        .map(|(summary, _)| summary))
}

pub(crate) fn head(
    tx: &Transaction<'_>,
    plan: &Plan<'_>,
    term: Option<&SearchTerm>,
) -> rusqlite::Result<Vec<HistoryRow>> {
    let open = summaries(tx, &base::history_head(), [plan.account])?;
    let mut rows = Vec::new();
    for (summary, _) in open {
        let id: i64 = summary.id.parse().unwrap_or_default();
        let labels = match term {
            Some(_) => texts(tx, sql::MEMBER_LABELS, id)?,
            None => Vec::new(),
        };
        let row = row(tx, summary, None)?;
        if head_accepts(plan, &row, term, &labels) {
            rows.push(row);
        }
    }
    Ok(rows)
}

pub(crate) fn report_id_matches(
    tx: &Transaction<'_>,
    scope: AccountScope,
    term: Option<&SearchTerm>,
) -> rusqlite::Result<Vec<i64>> {
    let Some(prefix) = term.and_then(|term| term.report_id_prefix.clone()) else {
        return Ok(Vec::new());
    };
    tx.prepare_cached(sql::REPORT_ID_MATCHES)?
        .query_map(rusqlite::params![scope.0.0, prefix], |row| row.get(0))?
        .collect()
}

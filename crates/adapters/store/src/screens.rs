use mivia_domain::ids::Timestamp;
use mivia_domain::output::ReportFolder;
use mivia_domain::path::WinPath;
use mivia_ports::ipc::screens::{
    AnalysisView, AttentionSeries, HoldKind, HoldView, NeedsAttention, ProblemImage,
    ReportPartView, ReportView,
};
use mivia_ports::ipc::views::{BatchSummary, FolderView, OriginView};
use mivia_ports::store::AccountScope;
use mivia_ports::store::intake::MarkerPeer;
use mivia_ports::time::iso_utc;
use rusqlite::{OptionalExtension as _, Row, Transaction, params};

use crate::codec::{parse_hold_reason, report_folder};
use crate::queries::summaries;
use crate::sql::{self, screens};

fn conversion(error: impl std::error::Error + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(error))
}

struct FolderRow {
    id: i64,
    path: String,
    position: u32,
    active: bool,
    missing: bool,
    quiet_ms: i64,
    report: (String, Option<String>, Option<String>),
    waiting_since: Option<i64>,
    marker: Option<MarkerPeer>,
}

fn folder_row(row: &Row<'_>) -> rusqlite::Result<FolderRow> {
    Ok(FolderRow {
        id: row.get(0)?,
        path: row.get(1)?,
        position: row.get(2)?,
        active: row.get(3)?,
        missing: row.get::<_, Option<i64>>(4)?.is_some(),
        quiet_ms: row.get(5)?,
        report: (row.get(6)?, row.get(7)?, row.get(8)?),
        waiting_since: row.get(9)?,
        marker: row
            .get::<_, Option<String>>(10)?
            .and_then(|json| serde_json::from_str(&json).ok()),
    })
}

fn report_target(
    root: &WinPath,
    report: &(String, Option<String>, Option<String>),
) -> Option<String> {
    match report_folder(&report.0, report.1.clone(), report.2.as_deref()).ok()? {
        ReportFolder::BesideSeries => None,
        folder @ (ReportFolder::Subfolder(_) | ReportFolder::Custom(_)) => {
            folder.target(root, &[]).ok().map(|path| path.to_string())
        }
    }
}

fn folder_view(
    tx: &Transaction<'_>,
    scope: AccountScope,
    row: FolderRow,
    open: &[(BatchSummary, i64)],
) -> rusqlite::Result<FolderView> {
    let id = row.id.to_string();
    let root = WinPath::parse(&row.path).map_err(|_| conversion(std::fmt::Error))?;
    let last = summaries(
        tx,
        &screens::last_closed_of_folder(),
        params![scope.0.0, row.id],
    )?;
    let open_series = open
        .iter()
        .find(|(series, _)| matches!(&series.origin, OriginView::Folder { folder_id } if *folder_id == id))
        .map(|(series, _)| series.clone());
    let last_series = last.into_iter().next().map(|(series, _)| series);
    let latest_report = match &last_series {
        Some(series) => latest_part(tx, series)?,
        None => None,
    };
    Ok(FolderView {
        name: root.file_name().unwrap_or(&row.path).to_owned(),
        report_folder: report_target(&root, &row.report),
        path: row.path,
        id,
        position: row.position,
        paused: !row.active,
        missing: row.missing,
        waiting_since: row
            .waiting_since
            .filter(|_| !row.missing)
            .map(|at| iso_utc(Timestamp(at))),
        quiet_seconds: u32::try_from(row.quiet_ms / 1000).unwrap_or(u32::MAX),
        open_series,
        last_series,
        latest_report,
    })
}

fn latest_part(tx: &Transaction<'_>, series: &BatchSummary) -> rusqlite::Result<Option<String>> {
    let id: Option<i64> = tx
        .prepare_cached(screens::LATEST_SAVED_PART)?
        .query_row([series.id.parse::<i64>().unwrap_or(0)], |row| row.get(0))?;
    Ok(id.map(|id| id.to_string()))
}

fn folder_rows(tx: &Transaction<'_>, scope: AccountScope) -> rusqlite::Result<Vec<FolderRow>> {
    tx.prepare_cached(screens::FOLDER_VIEWS)?
        .query_map([scope.0.0], folder_row)?
        .collect()
}

pub(crate) fn folders(
    tx: &Transaction<'_>,
    scope: AccountScope,
) -> rusqlite::Result<Vec<FolderView>> {
    let rows = folder_rows(tx, scope)?;
    let open = summaries(tx, &sql::history_head(), [scope.0.0])?;
    rows.into_iter()
        .map(|row| folder_view(tx, scope, row, &open))
        .collect()
}

pub(crate) fn holds(tx: &Transaction<'_>, scope: AccountScope) -> rusqlite::Result<Vec<HoldView>> {
    let rows = tx
        .prepare_cached(screens::HOLDS)?
        .query_map([scope.0.0], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|(reason, since)| {
            let reason = parse_hold_reason(&reason).map_err(|_| conversion(std::fmt::Error))?;
            Ok(HoldView {
                kind: HoldKind::from(reason),
                since: iso_utc(Timestamp(since)),
            })
        })
        .collect()
}

fn series_item(tx: &Transaction<'_>, summary: &BatchSummary) -> rusqlite::Result<NeedsAttention> {
    let series = AttentionSeries {
        id: summary.id.clone(),
        label: summary.label.clone(),
    };
    let part = tx.prepare_cached(screens::PROBLEM_PART)?.query_row(
        [summary.id.parse::<i64>().unwrap_or(0)],
        |row| {
            Ok((
                row.get::<_, Option<i64>>(0)?,
                row.get::<_, Option<String>>(1)?,
            ))
        },
    )?;
    Ok(match part {
        (Some(id), Some(state)) if state == "not_saved" => NeedsAttention::ReportNotSaved {
            series,
            part_id: id.to_string(),
        },
        (Some(id), _) => NeedsAttention::ReportFailed {
            series,
            part_id: id.to_string(),
        },
        (None, _) if summary.counts.reports_stale > 0 => NeedsAttention::ReportStale { series },
        (None, _) => NeedsAttention::SeriesProblems {
            first: first_problem(tx, &series.id)?,
            series,
            problems: summary.counts.problems,
        },
    })
}

fn first_problem(tx: &Transaction<'_>, series: &str) -> rusqlite::Result<Option<ProblemImage>> {
    tx.prepare_cached(screens::FIRST_PROBLEM_IMAGE)?
        .query_row([series.parse::<i64>().unwrap_or(0)], |row| {
            Ok(ProblemImage {
                item_id: row.get::<_, i64>(0)?.to_string(),
                name: row.get(1)?,
                state: row.get(2)?,
            })
        })
        .optional()
}

fn folder_attention(row: FolderRow) -> Option<NeedsAttention> {
    let name = WinPath::parse(&row.path)
        .ok()
        .and_then(|path| path.file_name().map(str::to_owned))
        .unwrap_or_else(|| row.path.clone());
    if row.missing {
        return Some(NeedsAttention::FolderNotFound {
            folder_id: row.id.to_string(),
            folder_name: name,
            unreachable_since: row.waiting_since.map(|at| iso_utc(Timestamp(at))),
            path: row.path,
        });
    }
    let peer = row.marker.filter(|_| row.active)?;
    Some(NeedsAttention::WatchedElsewhere {
        folder_id: row.id.to_string(),
        folder_name: name,
        machine: peer.machine,
        user: peer.user,
    })
}

pub(crate) fn attention(
    tx: &Transaction<'_>,
    scope: AccountScope,
) -> rusqlite::Result<(Vec<NeedsAttention>, u32)> {
    let missing: Vec<NeedsAttention> = folder_rows(tx, scope)?
        .into_iter()
        .filter_map(folder_attention)
        .collect();
    let series = summaries(tx, &sql::attention_series(), [scope.0.0])?;
    let counted: u32 = tx.query_row(sql::ATTENTION_COUNT, [scope.0.0], |row| row.get(0))?;
    let total = counted + u32::try_from(missing.len()).unwrap_or(u32::MAX);
    let mut items = missing;
    for (summary, _) in &series {
        items.push(series_item(tx, summary)?);
    }
    items.sort_by_key(|item| {
        !matches!(
            item,
            NeedsAttention::FolderNotFound { .. }
                | NeedsAttention::WatchedElsewhere { .. }
                | NeedsAttention::ReportNotSaved { .. }
                | NeedsAttention::ReportFailed { .. }
        )
    });
    Ok((items, total))
}

pub(crate) fn analyses(tx: &Transaction<'_>, batch: i64) -> rusqlite::Result<Vec<AnalysisView>> {
    tx.prepare_cached(screens::BATCH_ANALYSES)?
        .query_map([batch], |row| {
            let names: String = row.get(2)?;
            Ok(AnalysisView {
                key: row.get(0)?,
                model_id: row.get(1)?,
                names: serde_json::from_str(&names).unwrap_or_default(),
            })
        })?
        .collect()
}

fn part(row: &Row<'_>) -> rusqlite::Result<ReportPartView> {
    let file: Option<String> = row.get(8)?;
    Ok(ReportPartView {
        id: row.get::<_, i64>(0)?.to_string(),
        part_no: row.get(1)?,
        part_count: row.get(2)?,
        format: row.get(3)?,
        member_from: row.get(4)?,
        member_to: row.get(5)?,
        state: row.get(6)?,
        reason: row.get(7)?,
        file_name: file.and_then(|path| WinPath::parse(&path).ok()?.file_name().map(str::to_owned)),
    })
}

fn report_head(row: &Row<'_>) -> rusqlite::Result<ReportView> {
    Ok(ReportView {
        id: row.get::<_, i64>(0)?.to_string(),
        analysis_key: row.get(1)?,
        state: row.get(2)?,
        stale: row.get(3)?,
        replaced_by: row.get::<_, Option<i64>>(4)?.map(|other| other.to_string()),
        moved: row.get(5)?,
        parts: Vec::new(),
    })
}

pub(crate) fn reports(
    tx: &Transaction<'_>,
    scope: AccountScope,
    batch: i64,
) -> rusqlite::Result<Vec<ReportView>> {
    let heads = tx
        .prepare_cached(screens::SERIES_REPORTS)?
        .query_map(params![scope.0.0, batch], report_head)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut parts = tx.prepare_cached(screens::REPORT_PARTS)?;
    heads
        .into_iter()
        .map(|head| {
            let id: i64 = head.id.parse().unwrap_or_default();
            Ok(ReportView {
                parts: parts
                    .query_map([id], part)?
                    .collect::<rusqlite::Result<_>>()?,
                ..head
            })
        })
        .collect()
}

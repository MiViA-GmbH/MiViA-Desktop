use async_trait::async_trait;
use mivia_domain::ids::{BatchId, Timestamp};
use mivia_ports::ipc::screens::{AccountView, HoldsView, SeriesReportsView};
use mivia_ports::ipc::views::{
    BatchCounts, BatchItem, BatchItemsPage, BatchSummary, BatchView, ComputationItem, FoldersView,
    HistoryCursor, HistoryPage, NeedsAttentionView, OriginView,
};
use mivia_ports::queries::Queries;
use mivia_ports::store::{AccountScope, SettingsScope, StoreError};
use mivia_ports::time::iso_utc;
use rusqlite::{OptionalExtension, Row, Transaction, params};

use crate::screens;
use crate::sql;
use crate::writer::SqliteStore;

fn summary(row: &Row<'_>) -> rusqlite::Result<(BatchSummary, i64)> {
    let id: i64 = row.get(0)?;
    let folder: Option<i64> = row.get(3)?;
    let closed_at: Option<i64> = row.get(6)?;
    let origin = folder.map_or(OriginView::OneOff, |folder| OriginView::Folder {
        folder_id: folder.to_string(),
    });
    let summary = BatchSummary {
        id: id.to_string(),
        label: row.get(1)?,
        origin,
        open: row.get(4)?,
        opened_at: iso_utc(Timestamp(row.get(5)?)),
        closed_at: closed_at.map(|at| iso_utc(Timestamp(at))),
        counts: BatchCounts {
            files: row.get(7)?,
            done: row.get(8)?,
            failed: row.get(9)?,
            problems: row.get(10)?,
            active: row.get(11)?,
            reports_saved: row.get(12)?,
            reports_problem: row.get(13)?,
            reports_total: row.get(14)?,
            reports_stale: row.get(15)?,
        },
        needs_attention: row.get(16)?,
        closure_reason: row.get(18)?,
    };
    Ok((summary, row.get(17)?))
}

pub(crate) fn summaries(
    tx: &Transaction<'_>,
    query: &str,
    values: impl rusqlite::Params,
) -> rusqlite::Result<Vec<(BatchSummary, i64)>> {
    tx.prepare_cached(query)?
        .query_map(values, summary)?
        .collect()
}

fn history(
    tx: &Transaction<'_>,
    scope: AccountScope,
    after: (i64, i64),
) -> rusqlite::Result<(Vec<BatchSummary>, Option<HistoryCursor>)> {
    let mut rows = summaries(
        tx,
        &sql::history_body(),
        params![scope.0.0, after.0, after.1],
    )?;
    let page = usize::try_from(sql::PAGE_SIZE).unwrap_or(usize::MAX);
    let next = (rows.len() > page)
        .then(|| rows.truncate(page))
        .and_then(|()| rows.last())
        .map(|(row, sort_at)| HistoryCursor {
            sort_at: sort_at.to_string(),
            id: row.id.clone(),
        });
    Ok((rows.into_iter().map(|(row, _)| row).collect(), next))
}

fn cursor(after: Option<&HistoryCursor>) -> Option<(i64, i64)> {
    let after = after?;
    Some((after.sort_at.parse().ok()?, after.id.parse().ok()?))
}

fn items(
    tx: &Transaction<'_>,
    scope: AccountScope,
    batch: BatchId,
    after: u32,
) -> rusqlite::Result<(Vec<BatchItem>, Option<u32>)> {
    let mut files = tx.prepare_cached(&sql::batch_items())?;
    let rows = files
        .query_map(params![scope.0.0, batch.0, after], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get::<_, u32>(3)?,
                row.get(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<(i64, String, String, u32, Option<String>)>>>()?;
    let mut computations = tx.prepare_cached(sql::ITEM_COMPUTATIONS)?;
    let page = usize::try_from(sql::PAGE_SIZE).unwrap_or(usize::MAX);
    let next = rows
        .get(page.saturating_sub(1))
        .filter(|_| rows.len() > page)
        .map(|row| row.3);
    let mut out = Vec::with_capacity(rows.len().min(page));
    for (id, label, state, _, reason) in rows.into_iter().take(page) {
        let computations = computations
            .query_map([id], |row| {
                Ok(ComputationItem {
                    id: row.get::<_, i64>(0)?.to_string(),
                    model_choice_key: row.get(1)?,
                    state: row.get(2)?,
                    reason: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        out.push(BatchItem {
            id: id.to_string(),
            label,
            state,
            reason,
            computations,
        });
    }
    Ok((out, next))
}

#[async_trait]
impl Queries for SqliteStore {
    async fn list_history(
        &self,
        scope: AccountScope,
        after: Option<HistoryCursor>,
    ) -> Result<HistoryPage, StoreError> {
        let start = cursor(after.as_ref());
        self.readers
            .read(move |tx, revision| {
                let head = match start {
                    Some(_) => Vec::new(),
                    None => summaries(tx, &sql::history_head(), [scope.0.0])?,
                };
                let (rows, next) = history(tx, scope, start.unwrap_or((i64::MAX, i64::MAX)))?;
                Ok(HistoryPage {
                    revision,
                    head: head.into_iter().map(|(row, _)| row).collect(),
                    rows,
                    next,
                })
            })
            .await
    }

    async fn list_needs_attention(
        &self,
        scope: AccountScope,
    ) -> Result<NeedsAttentionView, StoreError> {
        self.readers
            .read(move |tx, revision| {
                let (items, total) = screens::attention(tx, scope)?;
                Ok(NeedsAttentionView {
                    revision,
                    items,
                    total,
                })
            })
            .await
    }

    async fn get_batch(
        &self,
        scope: AccountScope,
        id: BatchId,
    ) -> Result<Option<BatchView>, StoreError> {
        self.readers
            .read(move |tx, revision| {
                let found = tx
                    .prepare_cached(&sql::batch_by_id())?
                    .query_row([scope.0.0, id.0], summary)
                    .optional()?;
                let analyses = screens::analyses(tx, id.0)?;
                Ok(found.map(|(summary, _)| BatchView {
                    revision,
                    summary,
                    analyses,
                }))
            })
            .await
    }

    async fn list_batch_items(
        &self,
        scope: AccountScope,
        id: BatchId,
        after: Option<u32>,
    ) -> Result<BatchItemsPage, StoreError> {
        self.readers
            .read(move |tx, revision| {
                let (items, next) = items(tx, scope, id, after.unwrap_or(0))?;
                Ok(BatchItemsPage {
                    revision,
                    items,
                    next,
                })
            })
            .await
    }

    async fn list_folders(&self, scope: AccountScope) -> Result<FoldersView, StoreError> {
        self.readers
            .read(move |tx, revision| {
                let folders = screens::folders(tx, scope)?;
                Ok(FoldersView { revision, folders })
            })
            .await
    }

    async fn list_holds(&self, scope: AccountScope) -> Result<HoldsView, StoreError> {
        self.readers
            .read(move |tx, revision| {
                let holds = screens::holds(tx, scope)?;
                Ok(HoldsView { revision, holds })
            })
            .await
    }

    async fn list_series_reports(
        &self,
        scope: AccountScope,
        id: BatchId,
    ) -> Result<SeriesReportsView, StoreError> {
        self.readers
            .read(move |tx, revision| {
                let reports = screens::reports(tx, scope, id.0)?;
                Ok(SeriesReportsView { revision, reports })
            })
            .await
    }

    async fn get_account(&self, scope: AccountScope) -> Result<Option<AccountView>, StoreError> {
        self.readers
            .read(move |tx, _| {
                tx.prepare_cached(sql::screens::ACCOUNT)?
                    .query_row([scope.0.0], |row| {
                        Ok(AccountView {
                            display_name: row.get(0)?,
                            server_url: row.get(1)?,
                            server_account_id: row.get(2)?,
                            email: row.get(3)?,
                        })
                    })
                    .optional()
            })
            .await
    }

    async fn settings(&self, scope: SettingsScope) -> Result<Option<String>, StoreError> {
        self.readers
            .read(move |tx, _| {
                tx.query_row(sql::screens::SETTINGS, [scope.name()], |row| row.get(0))
                    .optional()
            })
            .await
    }
}

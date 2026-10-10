use mivia_domain::ids::{BatchId, FolderId, Timestamp};
use mivia_domain::intake::BatchKey;
use mivia_domain::natural::sort_key;
use mivia_domain::path::WinPath;
use mivia_domain::source_file::SourceFileState;
use mivia_ports::store::TxError;
use mivia_ports::store::intake::OpenBatchRow;
use rusqlite::{Row, params};

use super::SqlTx;
use super::counters;
use super::intake::Seed;
use crate::codec::parse_path;
use crate::errors::SqlResult;
use crate::sql;

fn open_row(row: &Row<'_>) -> rusqlite::Result<OpenBatchRow> {
    Ok(OpenBatchRow {
        id: BatchId(row.get(0)?),
        folder: FolderId(row.get(1)?),
        key: row.get(2)?,
        opened_at: Timestamp(row.get(3)?),
        last_member_at: Timestamp(row.get(4)?),
        files: row.get(5)?,
        detected: row.get(6)?,
        quiet_ms: row.get(7)?,
    })
}

impl SqlTx<'_> {
    pub(super) fn open_batch_rows(&self) -> Result<Vec<OpenBatchRow>, TxError> {
        let mut statement = self.tx.prepare_cached(sql::OPEN_BATCH_ROWS).tx()?;
        statement
            .query_map([self.account()], open_row)
            .and_then(Iterator::collect)
            .tx()
    }

    pub(super) fn paths_in(&self, batch: BatchId) -> Result<Vec<WinPath>, TxError> {
        let texts: Vec<String> =
            self.column(sql::MEMBER_PATHS, params![batch.0, self.account()])?;
        texts.iter().map(|text| parse_path(text)).collect()
    }

    pub(super) fn set_label(&self, batch: BatchId, label: &str) -> Result<(), TxError> {
        self.tx
            .execute(
                "UPDATE batch SET label = ?3, label_sort = ?4 \
                 WHERE id = ?1 AND account_id = ?2 AND label_user_set = 0",
                params![batch.0, self.account(), label, sort_key(label)],
            )
            .tx()
            .map(drop)
    }

    pub(super) fn set_key(
        &self,
        batch: BatchId,
        key: &BatchKey,
        label: &str,
    ) -> Result<(), TxError> {
        self.tx
            .execute(
                "UPDATE batch SET batch_key = ?3 WHERE id = ?1 AND account_id = ?2 AND open = 1",
                params![batch.0, self.account(), key.key],
            )
            .tx()?;
        self.set_label(batch, label)
    }

    pub(super) fn carry_detected(&mut self, from: BatchId, now: Timestamp) -> Result<(), TxError> {
        let detected: Vec<i64> = self.column(sql::DETECTED_IN_BATCH, [from.0])?;
        if detected.is_empty() {
            return Ok(());
        }
        let (folder, key, label, app_version): (i64, String, String, String) = self
            .tx
            .query_row(
                "SELECT folder_id, batch_key, label, app_version FROM batch \
                 WHERE id = ?1 AND account_id = ?2",
                [from.0, self.account()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .tx()?;
        let to = self.open_series(&Seed {
            folder: FolderId(folder),
            key: &key,
            label: &label,
            opened_at: now,
            app_version: &app_version,
        })?;
        let moved = (
            counters::source_file(Some(SourceFileState::Detected)),
            counters::source_file(None),
        );
        for id in detected {
            self.tx.execute(sql::MOVE_MEMBER, [id, to.0]).tx()?;
            self.batch_changed(from.0, moved, now)?;
            self.batch_changed(to.0, (moved.1, moved.0), now)?;
        }
        self.touch_batch(to.0, now)
    }
}

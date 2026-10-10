use mivia_domain::batch::{self, ClosureReason, OPEN_SORT_AT};
use mivia_domain::ids::{BatchId, Timestamp};
use mivia_domain::natural::sort_key;
use mivia_domain::transition::IllegalTransition;
use mivia_ports::ipc::change::Change;
use mivia_ports::store::{BatchModelChoice, BatchRepo, NewBatch, Origin, TxError};
use rusqlite::{OptionalExtension, Transaction, params};

use super::SqlTx;
use crate::codec::closure_reason;
use crate::errors::SqlResult;
use crate::sql;

const INSERT_BATCH: &str = "INSERT INTO batch (account_id, origin, folder_id, batch_key, label, \
     label_sort, open, opened_at, sort_at, report_settings, app_version) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8, ?9, ?10) RETURNING id";

const INSERT_CHOICE: &str = "INSERT INTO batch_model_choice (batch_id, account_id, \
     model_choice_key, model_id, customization_id, names, model_version, sort_at) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)";

impl SqlTx<'_> {
    fn check_folder(&self, origin: Origin) -> Result<Option<i64>, TxError> {
        let Origin::Folder(folder) = origin else {
            return Ok(None);
        };
        self.tx
            .query_row(
                "SELECT id FROM watched_folder WHERE id = ?1 AND account_id = ?2",
                [folder.0, self.account()],
                |row| row.get(0),
            )
            .map(Some)
            .tx()
    }

    fn is_open(&self, id: BatchId) -> Result<bool, TxError> {
        self.tx
            .query_row(
                "SELECT open FROM batch WHERE id = ?1 AND account_id = ?2",
                [id.0, self.account()],
                |row| row.get(0),
            )
            .tx()
    }
}

impl SqlTx<'_> {
    fn insert_choices(&self, batch_id: i64, choices: &[BatchModelChoice]) -> Result<(), TxError> {
        let mut insert = self.tx.prepare_cached(INSERT_CHOICE).tx()?;
        for choice in choices {
            insert
                .execute(params![
                    batch_id,
                    self.account(),
                    choice.choice.key(),
                    choice.choice.model_id,
                    choice.choice.customization_id,
                    choice.names_json,
                    choice.model_version,
                    OPEN_SORT_AT
                ])
                .tx()?;
        }
        Ok(())
    }
}

impl BatchRepo for SqlTx<'_> {
    fn open(&mut self, batch: &NewBatch) -> Result<BatchId, TxError> {
        let folder = self.check_folder(batch.origin)?;
        let origin = if folder.is_some() {
            "folder"
        } else {
            "one_off"
        };
        let id: i64 = self
            .tx
            .prepare_cached(INSERT_BATCH)
            .and_then(|mut statement| {
                statement.query_row(
                    params![
                        self.account(),
                        origin,
                        folder,
                        batch.batch_key,
                        batch.label,
                        sort_key(&batch.label),
                        batch.opened_at.0,
                        OPEN_SORT_AT,
                        batch.report_settings_json,
                        batch.app_version
                    ],
                    |row| row.get(0),
                )
            })
            .tx()?;
        self.insert_choices(id, &batch.model_choices)?;
        self.record(Change::History);
        Ok(BatchId(id))
    }

    fn close(
        &mut self,
        id: BatchId,
        reason: ClosureReason,
        closed_at: Timestamp,
    ) -> Result<(), TxError> {
        if !self.is_open(id)? {
            return Err(TxError::Illegal(IllegalTransition("batch")));
        }
        let latest: Option<i64> = self
            .tx
            .query_row("SELECT max(sort_at) FROM batch WHERE open = 0", [], |row| {
                row.get(0)
            })
            .tx()?;
        let sort_at = batch::closed_sort_at(closed_at, latest);
        self.tx
            .execute(
                "UPDATE batch SET open = 0, closure_reason = ?2, closed_at = ?3, sort_at = ?4 \
                 WHERE id = ?1",
                params![id.0, closure_reason(reason), closed_at.0, sort_at],
            )
            .tx()?;
        self.tx
            .execute(
                "UPDATE batch_model_choice SET sort_at = ?2 WHERE batch_id = ?1",
                [id.0, sort_at],
            )
            .tx()?;
        write_search_doc(self.tx, id.0).tx()?;
        self.insert_waiting(id.0, closed_at)?;
        self.record(Change::History);
        Ok(())
    }

    fn open_batches(&mut self) -> Result<Vec<BatchId>, TxError> {
        let ids = self.column(sql::BATCH_OPEN, [self.account()])?;
        Ok(ids.into_iter().map(BatchId).collect())
    }
}

fn write_search_doc(tx: &Transaction<'_>, batch_id: i64) -> rusqlite::Result<()> {
    let Some((label, sort_at)) = tx
        .query_row(
            "SELECT label, sort_at FROM batch WHERE id = ?1 AND open = 0",
            [batch_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?
    else {
        return Ok(());
    };
    let mut members = tx
        .prepare_cached("SELECT label FROM source_file WHERE batch_id = ?1 ORDER BY arrival_seq")?;
    let labels = members
        .query_map([batch_id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let text = std::iter::once(label)
        .chain(labels)
        .collect::<Vec<_>>()
        .join("\n");
    tx.execute("DELETE FROM batch_search WHERE rowid = ?1", [sort_at])?;
    tx.execute(
        "INSERT INTO batch_search (rowid, text) VALUES (?1, ?2)",
        params![sort_at, text],
    )?;
    Ok(())
}

pub(crate) fn rebuild_search(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT INTO batch_search (batch_search) VALUES ('delete-all')",
        [],
    )?;
    let ids = tx
        .prepare("SELECT id FROM batch WHERE open = 0 ORDER BY sort_at")?
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ids.into_iter().try_for_each(|id| write_search_doc(tx, id))
}

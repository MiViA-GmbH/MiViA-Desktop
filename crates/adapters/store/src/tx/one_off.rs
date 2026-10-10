use mivia_domain::ids::Timestamp;
use mivia_ports::ipc::change::Change;
use mivia_ports::store::{AccountScope, SentImages, TxError};
use rusqlite::{OptionalExtension, params};

use super::SqlTx;
use crate::errors::SqlResult;

fn list(json: &str) -> Result<Vec<String>, TxError> {
    serde_json::from_str(json).map_err(|_| TxError::Corrupt(format!("pending one-off {json}")))
}

fn text(items: &[String]) -> String {
    serde_json::to_string(items).unwrap_or_else(|_| "[]".to_owned())
}

impl SqlTx<'_> {
    pub(super) fn read_pending(&self, scope: AccountScope) -> Result<Option<SentImages>, TxError> {
        let row: Option<(String, String)> = self
            .tx
            .query_row(
                "SELECT spool_ids, paths FROM pending_one_off WHERE account_id = ?1",
                [scope.0.0],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .tx()?;
        row.map(|(spools, paths)| {
            Ok(SentImages {
                spool_ids: list(&spools)?,
                paths: list(&paths)?,
            })
        })
        .transpose()
    }

    pub(super) fn write_pending(
        &mut self,
        scope: AccountScope,
        pending: Option<&SentImages>,
        now: Timestamp,
    ) -> Result<(), TxError> {
        self.run(
            "DELETE FROM pending_one_off WHERE account_id = ?1",
            [scope.0.0],
        )?;
        if let Some(pending) = pending {
            self.run(
                "INSERT INTO pending_one_off (account_id, spool_ids, paths, created_at) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    scope.0.0,
                    text(&pending.spool_ids),
                    text(&pending.paths),
                    now.0
                ],
            )?;
        }
        self.record(Change::OneOff);
        Ok(())
    }
}

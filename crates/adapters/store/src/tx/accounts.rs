use mivia_domain::hold::{Access, Gate, HoldReason, SERVICE_UNAVAILABLE_POLL_MS};
use mivia_domain::ids::{AccountId, FolderId, Timestamp};
use mivia_ports::ipc::change::Change;
use mivia_ports::store::{AccountRepo, FolderRepo, HoldRepo, NewAccount, NewFolder, TxError};
use rusqlite::params;
use rusqlite::types::FromSql;

use super::SqlTx;
use crate::codec::{hold_reason, label_rule, parse_hold_reason};
use crate::errors::SqlResult;
use crate::sql;

impl AccountRepo for SqlTx<'_> {
    fn upsert(&mut self, account: &NewAccount) -> Result<AccountId, TxError> {
        self.tx
            .prepare_cached(
                "INSERT INTO account (server_url, server_account_id, display_name) \
                 VALUES (?1, ?2, ?3) ON CONFLICT (server_url, server_account_id) \
                 DO UPDATE SET display_name = excluded.display_name RETURNING id",
            )
            .and_then(|mut statement| {
                statement.query_row(
                    params![
                        account.server_url,
                        account.server_account_id,
                        account.display_name
                    ],
                    |row| row.get(0),
                )
            })
            .map(AccountId)
            .tx()
    }

    fn activate(&mut self, id: AccountId) -> Result<(), TxError> {
        self.tx
            .execute("UPDATE account SET active = 0 WHERE active = 1", [])
            .tx()?;
        let updated = self
            .tx
            .execute("UPDATE account SET active = 1 WHERE id = ?1", [id.0])
            .tx()?;
        self.record(Change::All);
        (updated == 1).then_some(()).ok_or(TxError::NotFound)
    }
}

impl HoldRepo for SqlTx<'_> {
    fn raise(
        &mut self,
        reason: HoldReason,
        since: Timestamp,
        next_probe_at: Option<Timestamp>,
    ) -> Result<(), TxError> {
        self.tx
            .prepare_cached(
                "INSERT INTO account_hold (account_id, reason, since, next_probe_at) \
                 VALUES (?1, ?2, ?3, ?4) ON CONFLICT (account_id, reason) \
                 DO UPDATE SET next_probe_at = excluded.next_probe_at",
            )
            .and_then(|mut statement| {
                statement.execute(params![
                    self.account(),
                    hold_reason(reason),
                    since.0,
                    next_probe_at.map(|at| at.0)
                ])
            })
            .tx()?;
        self.record(Change::Holds);
        Ok(())
    }

    fn release(&mut self, reason: HoldReason) -> Result<(), TxError> {
        self.tx
            .execute(
                "DELETE FROM account_hold WHERE account_id = ?1 AND reason = ?2",
                params![self.account(), hold_reason(reason)],
            )
            .tx()?;
        self.record(Change::Holds);
        Ok(())
    }

    fn active(&mut self) -> Result<Vec<HoldReason>, TxError> {
        let texts: Vec<String> = self.column(
            "SELECT reason FROM account_hold WHERE account_id = ?1 ORDER BY reason",
            params![self.account()],
        )?;
        texts.iter().map(|text| parse_hold_reason(text)).collect()
    }

    fn access(&mut self, gate: Gate) -> Result<Access, TxError> {
        let level: i64 = self
            .tx
            .prepare_cached(&sql::gate_access(gate))
            .and_then(|mut statement| statement.query_row([self.account()], |row| row.get(0)))
            .tx()?;
        Ok(match level {
            0 => Access::Allowed,
            1 => Access::Throttled {
                min_interval_ms: SERVICE_UNAVAILABLE_POLL_MS,
            },
            _ => Access::Blocked,
        })
    }

    fn due_for_probe(&mut self, now: Timestamp) -> Result<Vec<HoldReason>, TxError> {
        let texts: Vec<String> =
            self.column(sql::HOLD_DUE_PROBE, params![self.account(), now.0])?;
        texts.iter().map(|text| parse_hold_reason(text)).collect()
    }
}

impl SqlTx<'_> {
    pub(super) fn column<T: FromSql>(
        &self,
        query: &str,
        values: impl rusqlite::Params,
    ) -> Result<Vec<T>, TxError> {
        let mut statement = self.tx.prepare_cached(query).tx()?;
        statement
            .query_map(values, |row| row.get(0))
            .and_then(Iterator::collect)
            .tx()
    }
}

impl FolderRepo for SqlTx<'_> {
    fn add(&mut self, folder: &NewFolder) -> Result<FolderId, TxError> {
        let id = self
            .tx
            .prepare_cached(
                "INSERT INTO watched_folder (account_id, path, path_key, position, quiet_period, \
                 label_rule, report_settings) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) RETURNING id",
            )
            .and_then(|mut statement| {
                statement.query_row(
                    params![
                        self.account(),
                        folder.path.to_string(),
                        folder.path.key(),
                        folder.position,
                        folder.quiet_period_ms,
                        label_rule(folder.label_rule),
                        folder.report_settings_json
                    ],
                    |row| row.get(0),
                )
            })
            .map(FolderId)
            .tx()?;
        self.record(Change::Folder { id: id.to_string() });
        Ok(id)
    }
}

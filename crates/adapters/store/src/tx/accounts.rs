use mivia_domain::hold::{Access, Gate, HoldReason, SERVICE_UNAVAILABLE_POLL_MS};
use mivia_domain::ids::{AccountId, Timestamp};
use mivia_ports::ipc::change::Change;
use mivia_ports::store::{AccountRepo, HoldRepo, NewAccount, TxError};
use rusqlite::params;
use rusqlite::types::FromSql;

use super::SqlTx;
use crate::codec::{hold_reason, parse_hold_reason};
use crate::errors::SqlResult;
use crate::sql;

impl AccountRepo for SqlTx<'_> {
    fn upsert(&mut self, account: &NewAccount) -> Result<AccountId, TxError> {
        self.tx
            .prepare_cached(
                "INSERT INTO account (server_url, server_account_id, display_name, email, issued_key_id) \
                 VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT (server_url, server_account_id) \
                 DO UPDATE SET display_name = excluded.display_name, email = excluded.email, \
                 issued_key_id = excluded.issued_key_id \
                 RETURNING id",
            )
            .and_then(|mut statement| {
                statement.query_row(
                    params![
                        account.server_url,
                        account.server_account_id,
                        account.display_name,
                        account.email,
                        account.issued_key_id
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

    fn active(&mut self) -> Result<Option<AccountId>, TxError> {
        let ids: Vec<i64> = self.column("SELECT id FROM account WHERE active = 1", [])?;
        Ok(ids.first().copied().map(AccountId))
    }

    fn deactivate(&mut self) -> Result<(), TxError> {
        self.tx
            .execute("UPDATE account SET active = 0 WHERE active = 1", [])
            .tx()?;
        self.record(Change::All);
        Ok(())
    }

    fn find(
        &mut self,
        server_url: &str,
        server_account_id: &str,
    ) -> Result<Option<AccountId>, TxError> {
        let ids: Vec<i64> = self.column(
            "SELECT id FROM account WHERE server_url = ?1 AND server_account_id = ?2",
            params![server_url, server_account_id],
        )?;
        Ok(ids.first().copied().map(AccountId))
    }

    fn issued_key_id(
        &mut self,
        server_url: &str,
        server_account_id: &str,
    ) -> Result<Option<String>, TxError> {
        let ids: Vec<Option<String>> = self.column(
            "SELECT issued_key_id FROM account WHERE server_url = ?1 AND server_account_id = ?2",
            params![server_url, server_account_id],
        )?;
        Ok(ids.into_iter().next().flatten())
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

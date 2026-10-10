use mivia_ports::store::TxError;
use mivia_ports::store::telemetry::{
    DAY_MS, Dropped, NewTelemetryRow, TelemetryRepo, TelemetryRow, Unsent,
};
use rusqlite::{OptionalExtension, Row, params};

use super::SqlTx;
use crate::errors::SqlResult;

const COLUMNS: &str = "seq, session_id, account_id, recorded_at, name, category, priority, \
     payload, batch_id, sent_at";

fn row(row: &Row<'_>) -> rusqlite::Result<TelemetryRow> {
    Ok(TelemetryRow {
        seq: row.get(0)?,
        session_id: row.get(1)?,
        account_id: row.get(2)?,
        recorded_at: row.get(3)?,
        name: row.get(4)?,
        category: row.get(5)?,
        interaction: row.get::<_, i64>(6)? == 1,
        payload: row.get(7)?,
        batch_id: row.get(8)?,
        sent_at: row.get(9)?,
    })
}

fn overflow(unsent: Unsent, max_events: u64, max_bytes: u64) -> u64 {
    let by_events = unsent.events.saturating_sub(max_events);
    let by_bytes = unsent
        .bytes
        .saturating_sub(max_bytes)
        .saturating_mul(unsent.events)
        .div_ceil(unsent.bytes.max(1));
    by_events.max(by_bytes).max(1)
}

fn count(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn unsigned(value: i64) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

impl SqlTx<'_> {
    fn delete_oldest(&self, interaction: bool, limit: u64) -> Result<u64, TxError> {
        let deleted = self
            .tx
            .execute(
                "DELETE FROM telemetry_event WHERE seq IN (SELECT seq FROM telemetry_event \
                 WHERE sent_at IS NULL AND priority = ?1 ORDER BY seq LIMIT ?2)",
                params![i64::from(interaction), count(limit)],
            )
            .tx()?;
        Ok(deleted as u64)
    }

    fn insert_event(&self, seq: i64, new: &NewTelemetryRow) -> Result<(), TxError> {
        self.tx
            .prepare_cached(
                "INSERT INTO telemetry_event (seq, session_id, account_id, recorded_at, \
                 name, category, priority, payload) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )
            .and_then(|mut statement| {
                statement.execute(params![
                    seq,
                    new.session_id,
                    new.account_id,
                    new.recorded_at,
                    new.name,
                    new.category,
                    i64::from(new.interaction),
                    new.payload
                ])
            })
            .tx()
            .map(drop)
    }
}

impl TelemetryRepo for SqlTx<'_> {
    fn append(&mut self, rows: &[NewTelemetryRow], day_limit: u64) -> Result<Dropped, TxError> {
        let (mut seq, mut day, mut events): (i64, i64, i64) = self
            .tx
            .query_row(
                "SELECT seq, day, day_events FROM telemetry_counter WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .tx()?;
        let mut dropped = Dropped::default();
        for new in rows {
            let today = new.recorded_at.div_euclid(DAY_MS);
            if today != day {
                (day, events) = (today, 0);
            }
            if unsigned(events) >= day_limit {
                if new.interaction {
                    dropped.interaction += 1;
                } else {
                    dropped.core += 1;
                }
                continue;
            }
            seq += 1;
            events += 1;
            self.insert_event(seq, new)?;
        }
        self.tx
            .execute(
                "UPDATE telemetry_counter SET seq = ?1, day = ?2, day_events = ?3 WHERE id = 1",
                params![seq, day, events],
            )
            .tx()?;
        Ok(dropped)
    }

    fn enforce_caps(&mut self, max_events: u64, max_bytes: u64) -> Result<Dropped, TxError> {
        let mut dropped = Dropped::default();
        loop {
            let unsent = self.unsent()?;
            if unsent.events <= max_events && unsent.bytes <= max_bytes {
                return Ok(dropped);
            }
            let chunk = overflow(unsent, max_events, max_bytes);
            let interaction = self.delete_oldest(true, chunk)?;
            dropped.interaction += interaction;
            if interaction == 0 {
                let core = self.delete_oldest(false, chunk)?;
                dropped.core += core;
                if core == 0 {
                    return Ok(dropped);
                }
            }
        }
    }

    fn save_session(&mut self, session_id: &str, resource: &str) -> Result<(), TxError> {
        self.tx
            .execute(
                "INSERT INTO telemetry_session (session_id, resource) VALUES (?1, ?2) \
                 ON CONFLICT (session_id) DO UPDATE SET resource = ?2",
                params![session_id, resource],
            )
            .tx()?;
        Ok(())
    }

    fn session_resource(&mut self, session_id: &str) -> Result<Option<String>, TxError> {
        let found: Vec<String> = self.column(
            "SELECT resource FROM telemetry_session WHERE session_id = ?1",
            [session_id],
        )?;
        Ok(found.into_iter().next())
    }

    fn pending_batch(&mut self) -> Result<Vec<TelemetryRow>, TxError> {
        self.rows(
            &format!(
                "SELECT {COLUMNS} FROM telemetry_event WHERE sent_at IS NULL AND batch_id = \
                 (SELECT batch_id FROM telemetry_event WHERE sent_at IS NULL \
                 AND batch_id IS NOT NULL ORDER BY seq LIMIT 1) ORDER BY priority, seq"
            ),
            [],
            row,
        )
    }

    fn candidates(&mut self, limit: u32) -> Result<Vec<TelemetryRow>, TxError> {
        let oldest: Option<(String, Option<String>)> = self
            .tx
            .query_row(
                "SELECT session_id, account_id FROM telemetry_event \
                 WHERE sent_at IS NULL AND batch_id IS NULL ORDER BY seq LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .tx()?;
        let Some((session, account)) = oldest else {
            return Ok(Vec::new());
        };
        self.rows(
            &format!(
                "SELECT {COLUMNS} FROM telemetry_event WHERE sent_at IS NULL \
                 AND batch_id IS NULL AND session_id = ?1 AND account_id IS ?2 \
                 ORDER BY priority, seq LIMIT ?3"
            ),
            params![session, account, limit],
            row,
        )
    }

    fn assign_batch(&mut self, batch_id: &str, seqs: &[i64]) -> Result<(), TxError> {
        for seq in seqs {
            self.tx
                .execute(
                    "UPDATE telemetry_event SET batch_id = ?1 WHERE seq = ?2 AND sent_at IS NULL",
                    params![batch_id, seq],
                )
                .tx()?;
        }
        Ok(())
    }

    fn release_batch(&mut self, batch_id: &str) -> Result<(), TxError> {
        self.tx
            .execute(
                "UPDATE telemetry_event SET batch_id = NULL \
                 WHERE batch_id = ?1 AND sent_at IS NULL",
                [batch_id],
            )
            .tx()?;
        Ok(())
    }

    fn drop_rows(&mut self, seqs: &[i64]) -> Result<(), TxError> {
        for seq in seqs {
            self.tx
                .execute("DELETE FROM telemetry_event WHERE seq = ?1", [seq])
                .tx()?;
        }
        Ok(())
    }

    fn mark_sent(&mut self, batch_id: &str, at: i64) -> Result<(), TxError> {
        self.tx
            .execute(
                "UPDATE telemetry_event SET sent_at = ?2 WHERE batch_id = ?1 AND sent_at IS NULL",
                params![batch_id, at],
            )
            .tx()?;
        Ok(())
    }

    fn unsent(&mut self) -> Result<Unsent, TxError> {
        let (events, bytes, oldest_at): (i64, i64, Option<i64>) = self
            .tx
            .query_row(
                "SELECT count(*), ifnull(sum(length(payload)), 0), min(recorded_at) \
                 FROM telemetry_event WHERE sent_at IS NULL",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .tx()?;
        Ok(Unsent {
            events: unsigned(events),
            bytes: unsigned(bytes),
            oldest_at,
        })
    }

    fn delete_unsent(&mut self) -> Result<u64, TxError> {
        let deleted = self
            .tx
            .execute("DELETE FROM telemetry_event WHERE sent_at IS NULL", [])
            .tx()?;
        Ok(deleted as u64)
    }

    fn prune_sent(&mut self, before: i64) -> Result<u64, TxError> {
        let deleted = self
            .tx
            .execute(
                "DELETE FROM telemetry_event WHERE sent_at IS NOT NULL AND sent_at < ?1",
                [before],
            )
            .tx()?;
        Ok(deleted as u64)
    }

    fn delete_sent(&mut self) -> Result<u64, TxError> {
        let deleted = self
            .tx
            .execute("DELETE FROM telemetry_event WHERE sent_at IS NOT NULL", [])
            .tx()?;
        Ok(deleted as u64)
    }

    fn sent_since(&mut self, since: i64) -> Result<Vec<TelemetryRow>, TxError> {
        self.rows(
            &format!(
                "SELECT {COLUMNS} FROM telemetry_event WHERE sent_at >= ?1 \
                 ORDER BY sent_at DESC, priority, seq"
            ),
            [since],
            row,
        )
    }
}

use mivia_domain::ids::{SourceFileId, Timestamp};
use mivia_domain::source_file::{SourceFileEvent, SourceFileState};
use mivia_ports::ipc::change::Change;
use mivia_ports::store::{NewSourceFile, SourceClaim, SourceFileRepo, TxError};
use rusqlite::params;

use super::counters::{self, Counts};
use super::{SqlTx, id_list};
use crate::codec::{SOURCE_STATES, decode, source_state};
use crate::errors::SqlResult;
use crate::sql;

const INSERT_FILE: &str = "INSERT INTO source_file (account_id, batch_id, path, label, size, \
     mtime, state, arrival_seq, next_attempt_at) \
     SELECT ?1, b.id, ?3, ?4, ?5, ?6, 'detected', \
     COALESCE((SELECT max(arrival_seq) FROM source_file WHERE batch_id = b.id), 0) + 1, ?7 \
     FROM batch b WHERE b.id = ?2 AND b.account_id = ?1 RETURNING id";

const UPDATE_STATE: &str = "UPDATE source_file SET state = ?2, reason = ?3, \
     next_attempt_at = CASE WHEN ?2 IN ('detected', 'ready') THEN ?4 ELSE next_attempt_at END \
     WHERE id = ?1";

impl SqlTx<'_> {
    pub(super) fn batch_changed(
        &mut self,
        batch_id: i64,
        change: (Counts, Counts),
        now: Timestamp,
    ) -> Result<(), TxError> {
        counters::shift(self.tx, batch_id, change, now).tx()?;
        self.record(Change::BatchItems {
            batch_id: batch_id.to_string(),
        });
        if change.1.problems > change.0.problems {
            self.record(Change::NeedsAttention);
        }
        Ok(())
    }
}

impl SourceFileRepo for SqlTx<'_> {
    fn add(&mut self, file: &NewSourceFile) -> Result<SourceFileId, TxError> {
        let size = i64::try_from(file.size).map_err(|_| TxError::Conflict)?;
        let id: i64 = self
            .tx
            .prepare_cached(INSERT_FILE)
            .and_then(|mut statement| {
                statement.query_row(
                    params![
                        self.account(),
                        file.batch_id.0,
                        file.path.to_string(),
                        file.label,
                        size,
                        file.mtime.0,
                        file.next_attempt_at.0
                    ],
                    |row| row.get(0),
                )
            })
            .tx()?;
        let change = (
            counters::source_file(None),
            counters::source_file(Some(SourceFileState::Detected)),
        );
        self.batch_changed(file.batch_id.0, change, file.mtime)?;
        Ok(SourceFileId(id))
    }

    fn apply(
        &mut self,
        id: SourceFileId,
        event: SourceFileEvent,
        now: Timestamp,
    ) -> Result<SourceFileState, TxError> {
        let (batch_id, state, reason): (i64, String, Option<String>) = self
            .tx
            .query_row(
                "SELECT batch_id, state, reason FROM source_file WHERE id = ?1 AND account_id = ?2",
                [id.0, self.account()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .tx()?;
        let before = decode(
            "source file",
            &SOURCE_STATES,
            source_state,
            (&state, reason.as_deref()),
        )?;
        let after = before.apply(event)?;
        let (code, detail) = source_state(after);
        self.tx
            .prepare_cached(UPDATE_STATE)
            .and_then(|mut statement| statement.execute(params![id.0, code, detail, now.0]))
            .tx()?;
        let change = (
            counters::source_file(Some(before)),
            counters::source_file(Some(after)),
        );
        self.batch_changed(batch_id, change, now)?;
        Ok(after)
    }

    fn due(
        &mut self,
        claim: SourceClaim,
        now: Timestamp,
        exclude: &[SourceFileId],
    ) -> Result<Vec<SourceFileId>, TxError> {
        let query = match claim {
            SourceClaim::Probe => sql::source_due_probe(),
            SourceClaim::Upload => sql::source_due_upload(),
        };
        let ids = self.column(
            &query,
            params![
                self.account(),
                now.0,
                id_list(exclude, |id| id.0),
                sql::CLAIM_LIMIT
            ],
        )?;
        Ok(ids.into_iter().map(SourceFileId).collect())
    }
}

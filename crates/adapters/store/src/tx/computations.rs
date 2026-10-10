use mivia_domain::computation::{ComputationEvent, ComputationState};
use mivia_domain::ids::{ComputationId, Timestamp};
use mivia_ports::store::{ComputationRepo, NewComputation, TxError};
use rusqlite::params;

use super::counters;
use super::{SqlTx, id_list};
use crate::codec::{COMPUTATION_STATES, computation_state, decode};
use crate::errors::SqlResult;
use crate::sql;

const INSERT_COMPUTATION: &str = "INSERT INTO computation (account_id, batch_id, source_file_id, \
     model_choice_key, model_version, state, reason, reused_from, done_at, next_attempt_at, \
     app_version) SELECT ?1, s.batch_id, s.id, ?3, ?4, \
     CASE WHEN ?8 IS NULL THEN 'queued' ELSE 'done' END, \
     CASE WHEN ?8 IS NULL THEN NULL ELSE 'reused' END, ?8, \
     CASE WHEN ?8 IS NULL THEN NULL ELSE ?5 END, ?5, ?6 FROM source_file s \
     WHERE s.id = ?2 AND s.account_id = ?1 AND s.batch_id = ?7 RETURNING id";

const UPDATE_STATE: &str = "UPDATE computation SET state = ?2, reason = ?3, \
     next_attempt_at = CASE WHEN ?2 IN ('queued', 'submitting') THEN ?4 ELSE next_attempt_at END, \
     next_poll_at = CASE WHEN ?2 = 'computing' THEN ?4 ELSE next_poll_at END, \
     done_at = CASE WHEN ?2 = 'done' THEN ?4 ELSE done_at END \
     WHERE id = ?1";

impl ComputationRepo for SqlTx<'_> {
    fn add(&mut self, computation: &NewComputation) -> Result<ComputationId, TxError> {
        let id: i64 = self
            .tx
            .prepare_cached(INSERT_COMPUTATION)
            .and_then(|mut statement| {
                statement.query_row(
                    params![
                        self.account(),
                        computation.source_file_id.0,
                        computation.model_choice_key,
                        computation.model_version,
                        computation.next_attempt_at.0,
                        computation.app_version,
                        computation.batch_id.0,
                        computation.reused_from.map(|from| from.0)
                    ],
                    |row| row.get(0),
                )
            })
            .tx()?;
        let state = if computation.reused_from.is_some() {
            ComputationState::Done { reused: true }
        } else {
            ComputationState::Queued
        };
        let change = (
            counters::computation(None),
            counters::computation(Some(state)),
        );
        self.batch_changed(computation.batch_id.0, change, computation.next_attempt_at)?;
        Ok(ComputationId(id))
    }

    fn apply(
        &mut self,
        id: ComputationId,
        event: ComputationEvent,
        now: Timestamp,
    ) -> Result<ComputationState, TxError> {
        let (batch_id, state, reason): (i64, String, Option<String>) = self
            .tx
            .query_row(
                "SELECT batch_id, state, reason FROM computation WHERE id = ?1 AND account_id = ?2",
                [id.0, self.account()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .tx()?;
        let stored = (state.as_str(), reason.as_deref());
        let before = decode(
            "computation",
            &COMPUTATION_STATES,
            computation_state,
            stored,
        )?;
        let after = before.apply(event)?;
        let (code, detail) = computation_state(after);
        self.tx
            .prepare_cached(UPDATE_STATE)
            .and_then(|mut statement| statement.execute(params![id.0, code, detail, now.0]))
            .tx()?;
        let change = (
            counters::computation(Some(before)),
            counters::computation(Some(after)),
        );
        self.batch_changed(batch_id, change, now)?;
        Ok(after)
    }

    fn due_for_submit(
        &mut self,
        now: Timestamp,
        exclude: &[ComputationId],
    ) -> Result<Vec<ComputationId>, TxError> {
        self.computations_due(&sql::computation_due_submit(), now, exclude)
    }

    fn due_for_poll(
        &mut self,
        now: Timestamp,
        exclude: &[ComputationId],
    ) -> Result<Vec<ComputationId>, TxError> {
        self.computations_due(&sql::computation_due_poll(), now, exclude)
    }
}

impl SqlTx<'_> {
    fn computations_due(
        &self,
        query: &str,
        now: Timestamp,
        exclude: &[ComputationId],
    ) -> Result<Vec<ComputationId>, TxError> {
        let values = params![
            self.account(),
            now.0,
            id_list(exclude, |id| id.0),
            sql::CLAIM_LIMIT
        ];
        let ids = self.column(query, values)?;
        Ok(ids.into_iter().map(ComputationId).collect())
    }
}

use mivia_domain::ids::{SourceFileId, Timestamp};
use mivia_domain::source_file::SourceFileEvent;
use mivia_ports::ipc::change::Change;
use mivia_ports::store::pipeline::{CatalogueRow, Clamp};
use mivia_ports::store::{SourceFileRepo, TxError};
use rusqlite::params;

use super::SqlTx;
use crate::errors::SqlResult;
use crate::sql::pipeline as sql;

const CLAMP_FILES: &str = "UPDATE source_file SET next_attempt_at = ?2 WHERE account_id = ?1 \
     AND state IN ('detected', 'ready') AND next_attempt_at > ?2";
const CLAMP_COMPUTATIONS: &str = "UPDATE computation SET next_attempt_at = ?2 \
     WHERE account_id = ?1 AND state IN ('queued', 'submitting') AND next_attempt_at > ?2";
const CLAMP_POLLS: &str = "UPDATE computation SET next_poll_at = ?2 WHERE account_id = ?1 \
     AND state = 'computing' AND next_poll_at > ?2";
const CLAMP_PROBES: &str = "UPDATE account_hold SET next_probe_at = ?2 WHERE account_id = ?1 \
     AND next_probe_at > ?2";
const RECOVER_SUBMITS: &str = "UPDATE computation SET recovery_due = ?2 WHERE account_id = ?1 \
     AND state = 'submitting'";
const CATALOGUE_UNSEEN: &str = "UPDATE model_catalogue SET available = 0 WHERE account_id = ?1";
const CATALOGUE_SEEN: &str = "INSERT INTO model_catalogue (account_id, model_id, \
     customization_id, names, available, seen_at) VALUES (?1, ?2, ?3, ?4, 1, ?5) \
     ON CONFLICT (account_id, model_id, customization_id) DO UPDATE SET \
     names = excluded.names, available = 1, seen_at = excluded.seen_at";

impl SqlTx<'_> {
    pub(super) fn run(&self, query: &str, values: impl rusqlite::Params) -> Result<(), TxError> {
        self.tx
            .prepare_cached(query)
            .and_then(|mut statement| statement.execute(values))
            .tx()
            .map(drop)
    }

    pub(super) fn restart_rows(&mut self, now: Timestamp) -> Result<(), TxError> {
        let uploading: Vec<i64> = self.column(sql::UPLOADING_FILES, [self.account()])?;
        for id in uploading {
            let event = SourceFileEvent::UploadInterrupted;
            SourceFileRepo::apply(self, SourceFileId(id), event, now)?;
        }
        self.run(RECOVER_SUBMITS, params![self.account(), now.0])?;
        self.run(CLAMP_POLLS, params![self.account(), now.0])
    }

    pub(super) fn clamp_rows(&self, clamp: Clamp) -> Result<(), TxError> {
        let account = self.account();
        self.run(CLAMP_FILES, params![account, clamp.retry_until.0])?;
        self.run(CLAMP_COMPUTATIONS, params![account, clamp.retry_until.0])?;
        self.run(CLAMP_POLLS, params![account, clamp.poll_until.0])?;
        self.run(CLAMP_PROBES, params![account, clamp.probe_until.0])
    }

    pub(super) fn catalogue_rows(
        &mut self,
        rows: &[CatalogueRow],
        now: Timestamp,
    ) -> Result<(), TxError> {
        self.run(CATALOGUE_UNSEEN, [self.account()])?;
        for row in rows {
            let customization = row.customization_id.as_deref().unwrap_or_default();
            let values = params![
                self.account(),
                row.model_id,
                customization,
                row.names_json,
                now.0
            ];
            self.run(CATALOGUE_SEEN, values)?;
        }
        self.record(Change::Catalogue);
        Ok(())
    }
}

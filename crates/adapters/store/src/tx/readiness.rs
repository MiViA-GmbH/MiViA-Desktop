use mivia_domain::computation::ComputationEvent;
use mivia_domain::ids::{ComputationId, Fingerprint, FolderId, SourceFileId, Timestamp};
use mivia_domain::readiness::Attempts;
use mivia_domain::source_file::{Classification, SourceFileEvent};
use mivia_ports::fs::ImageRead;
use mivia_ports::store::intake::{Classified, ContentLookup, KnownPath, ProbeRow, Sighting};
use mivia_ports::store::{ComputationRepo, NewComputation, SourceFileRepo, TxError};
use rusqlite::{OptionalExtension, Row, params};

use super::SqlTx;
use super::ledger::known_path;
use crate::codec::parse_path;
use crate::errors::SqlResult;
use crate::sql;

struct ReadyRow {
    batch: i64,
    folder: FolderId,
    path: String,
    old: Option<Vec<u8>>,
    copy_of: Option<i64>,
}

type RawProbe = (i64, String, String, i64, Option<i64>);

fn probe_row(row: &Row<'_>) -> rusqlite::Result<RawProbe> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
    ))
}

fn ms(stat_value: u64) -> i64 {
    i64::try_from(stat_value).unwrap_or(i64::MAX)
}

impl SqlTx<'_> {
    pub(super) fn claim_probe_rows(
        &self,
        now: Timestamp,
        exclude: &[SourceFileId],
        limit: u32,
    ) -> Result<Vec<ProbeRow>, TxError> {
        let raws = self
            .tx
            .prepare_cached(sql::CLAIM_PROBES)
            .and_then(|mut statement| {
                let excluded = super::id_list(exclude, |id| id.0);
                statement
                    .query_map(params![self.account(), now.0, excluded, limit], probe_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .tx()?;
        raws.into_iter()
            .map(|(id, root, path, count, first)| {
                Ok(ProbeRow {
                    id: SourceFileId(id),
                    root: parse_path(&root)?,
                    path: parse_path(&path)?,
                    attempts: Attempts {
                        count: u32::try_from(count).unwrap_or(u32::MAX),
                        first_busy_at: first.map(Timestamp),
                    },
                })
            })
            .collect()
    }

    pub(super) fn set_retry(
        &self,
        id: SourceFileId,
        at: Timestamp,
        attempts: Attempts,
    ) -> Result<(), TxError> {
        self.tx
            .prepare_cached(
                "UPDATE source_file SET next_attempt_at = ?3, attempts = ?4, first_busy_at = ?5 \
                 WHERE id = ?1 AND account_id = ?2 AND state = 'detected'",
            )
            .and_then(|mut statement| {
                statement.execute(params![
                    id.0,
                    self.account(),
                    at.0,
                    attempts.count,
                    attempts.first_busy_at.map(|first| first.0)
                ])
            })
            .tx()
            .map(drop)
    }

    fn ready_row(&self, id: SourceFileId) -> Result<ReadyRow, TxError> {
        self.tx
            .prepare_cached(sql::READY_ROW)
            .and_then(|mut statement| {
                statement.query_row(params![id.0, self.account()], |row| {
                    Ok(ReadyRow {
                        batch: row.get(0)?,
                        folder: FolderId(row.get(1)?),
                        path: row.get(2)?,
                        old: row.get(3)?,
                        copy_of: row.get(4)?,
                    })
                })
            })
            .tx()
    }

    fn lookup(
        &self,
        id: SourceFileId,
        row: &ReadyRow,
        fingerprint: Fingerprint,
    ) -> Result<ContentLookup, TxError> {
        let print = fingerprint.0.as_slice();
        let earlier: Option<i64> = self
            .tx
            .prepare_cached(sql::EARLIER_IN_BATCH)
            .and_then(|mut statement| {
                statement
                    .query_row(params![row.batch, print, id.0], |r| r.get(0))
                    .optional()
            })
            .tx()?;
        let in_ledger = self
            .tx
            .prepare_cached(sql::IN_LEDGER)
            .and_then(|mut statement| {
                statement
                    .query_row(params![row.folder.0, print], |r| r.get::<_, i64>(0))
                    .optional()
            })
            .tx()?
            .is_some();
        let mut statement = self.tx.prepare_cached(sql::PATHS_WITH_CONTENT).tx()?;
        let paths = statement
            .query_map(params![row.folder.0, print], known_path)
            .and_then(Iterator::collect::<rusqlite::Result<Vec<_>>>)
            .tx()?;
        let (same, other): (Vec<KnownPath>, Vec<KnownPath>) = paths
            .into_iter()
            .map(super::ledger::RawKnown::decode)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .partition(|known| known.path.to_string() == row.path);
        Ok(ContentLookup {
            folder: row.folder,
            earlier_in_batch: earlier.map(SourceFileId),
            in_ledger,
            same_path: !same.is_empty(),
            other_paths: other,
        })
    }

    pub(super) fn mark_ready(
        &mut self,
        id: SourceFileId,
        read: &ImageRead,
        now: Timestamp,
    ) -> Result<Sighting, TxError> {
        let row = self.ready_row(id)?;
        SourceFileRepo::apply(self, id, SourceFileEvent::ReadinessPassed, now)?;
        let (fingerprint, stat) = (read.fingerprint, read.stat);
        let print = fingerprint.0.as_slice();
        self.tx
            .prepare_cached(sql::STORE_READ)
            .and_then(|mut statement| {
                statement.execute(params![
                    id.0,
                    print,
                    ms(stat.size),
                    stat.mtime.0,
                    stat.change_time.0
                ])
            })
            .tx()?;
        self.touch_batch(row.batch, now)?;
        if row.old.as_deref() == Some(print) {
            self.record_seen(&row, read, now)?;
            return Ok(Sighting::Unchanged {
                copy_of: row.copy_of.map(SourceFileId),
            });
        }
        if row.old.is_some() {
            self.drop_old_content(id, now)?;
        }
        let lookup = self.lookup(id, &row, fingerprint)?;
        self.record_seen(&row, read, now)?;
        Ok(Sighting::Fresh(lookup))
    }

    fn record_seen(&self, row: &ReadyRow, read: &ImageRead, now: Timestamp) -> Result<(), TxError> {
        self.tx
            .prepare_cached(sql::UPSERT_LEDGER_PATH)
            .and_then(|mut statement| {
                statement.execute(params![
                    row.folder.0,
                    row.path,
                    read.fingerprint.0.as_slice(),
                    ms(read.stat.size),
                    read.stat.mtime.0,
                    now.0
                ])
            })
            .tx()
            .map(drop)
    }

    fn drop_old_content(&mut self, id: SourceFileId, now: Timestamp) -> Result<(), TxError> {
        self.cancel_work(id, now)?;
        let done: Vec<i64> = self.column(sql::DONE_COMPUTATIONS_OF_FILE, [id.0])?;
        for computation in done {
            ComputationRepo::apply(
                self,
                ComputationId(computation),
                ComputationEvent::RunAgain,
                now,
            )?;
        }
        self.tx
            .execute(
                "UPDATE source_file SET copy_of = NULL WHERE id = ?1",
                [id.0],
            )
            .tx()
            .map(drop)
    }

    pub(super) fn classify_file(
        &mut self,
        id: SourceFileId,
        outcome: Classified,
        now: Timestamp,
    ) -> Result<(), TxError> {
        let class = match outcome {
            Classified::Copy(of) => {
                self.tx
                    .execute(
                        "UPDATE source_file SET copy_of = ?2 WHERE id = ?1",
                        [id.0, of.0],
                    )
                    .tx()?;
                Classification::CopyInBatch
            }
            Classified::KnownFile => Classification::KnownFile,
            Classified::KnownContent => Classification::KnownContent,
            Classified::New => Classification::New,
        };
        SourceFileRepo::apply(self, id, SourceFileEvent::Classified(class), now)?;
        if matches!(outcome, Classified::New | Classified::KnownContent) {
            self.create_computations(id, now)?;
        }
        Ok(())
    }

    fn create_computations(&mut self, id: SourceFileId, now: Timestamp) -> Result<(), TxError> {
        let plans = self
            .tx
            .prepare_cached(sql::COMPUTATION_PLAN)
            .and_then(|mut statement| {
                statement
                    .query_map(params![id.0], |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, Option<i64>>(3)?,
                            row.get::<_, bool>(4)?,
                        ))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .tx()?;
        for (batch, key, app_version, entry, entry_done) in plans {
            let reused_from = entry.filter(|_| entry_done).map(ComputationId);
            let computation = ComputationRepo::add(
                self,
                &NewComputation {
                    batch_id: mivia_domain::ids::BatchId(batch),
                    source_file_id: id,
                    model_choice_key: key.clone(),
                    model_version: None,
                    app_version,
                    next_attempt_at: now,
                    reused_from,
                },
            )?;
            if entry.is_none() {
                self.tx
                    .execute(sql::CLAIM_LEDGER, params![id.0, key, computation.0])
                    .tx()?;
            }
        }
        Ok(())
    }
}

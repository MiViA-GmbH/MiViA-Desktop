use mivia_domain::computation::ComputationEvent;
use mivia_domain::ids::{BatchId, ComputationId, FolderId, ModelChoice, SourceFileId, Timestamp};
use mivia_domain::source_file::{SourceFileEvent, SourceFileState};
use mivia_ports::store::intake::{Detected, Detection, Member};
use mivia_ports::store::{
    BatchModelChoice, BatchRepo, ComputationRepo, NewBatch, NewSourceFile, Origin, SourceFileRepo,
    TxError,
};
use rusqlite::{OptionalExtension, Row, params};

use super::SqlTx;
use crate::codec::{SOURCE_STATES, decode, parse_path, source_state};
use crate::errors::SqlResult;
use crate::sql;

pub(super) struct Seed<'a> {
    pub(super) folder: FolderId,
    pub(super) key: &'a str,
    pub(super) label: &'a str,
    pub(super) opened_at: Timestamp,
    pub(super) app_version: &'a str,
}

type RawMember = (i64, i64, String, String, Option<String>, i64, i64);

fn raw_member(row: &Row<'_>) -> rusqlite::Result<RawMember> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
    ))
}

fn member(raw: RawMember) -> Result<Member, TxError> {
    let (id, batch, path, state, reason, size, mtime) = raw;
    Ok(Member {
        id: SourceFileId(id),
        batch_id: BatchId(batch),
        path: parse_path(&path)?,
        state: decode(
            "source file",
            &SOURCE_STATES,
            source_state,
            (&state, reason.as_deref()),
        )?,
        size: u64::try_from(size).unwrap_or_default(),
        mtime: Timestamp(mtime),
    })
}

impl SqlTx<'_> {
    pub(super) fn live_members_of(&self, folder: FolderId) -> Result<Vec<Member>, TxError> {
        let raws = self
            .tx
            .prepare_cached(sql::LIVE_MEMBERS)
            .and_then(|mut statement| {
                statement
                    .query_map(params![self.account(), folder.0], raw_member)?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .tx()?;
        raws.into_iter().map(member).collect()
    }

    pub(super) fn touch_batch(&self, batch: i64, at: Timestamp) -> Result<(), TxError> {
        self.tx
            .prepare_cached(
                "UPDATE batch SET last_member_at = max(ifnull(last_member_at, 0), ?2) \
                 WHERE id = ?1 AND open = 1",
            )
            .and_then(|mut statement| statement.execute(params![batch, at.0]))
            .tx()
            .map(drop)
    }

    pub(super) fn batch_of(&self, id: SourceFileId) -> Result<i64, TxError> {
        self.tx
            .query_row(
                "SELECT batch_id FROM source_file WHERE id = ?1 AND account_id = ?2",
                [id.0, self.account()],
                |row| row.get(0),
            )
            .tx()
    }

    fn folder_choices(&self, folder: FolderId) -> Result<Vec<BatchModelChoice>, TxError> {
        let mut statement = self.tx.prepare_cached(sql::FOLDER_CHOICES).tx()?;
        statement
            .query_map([folder.0], |row| {
                Ok(BatchModelChoice {
                    choice: ModelChoice {
                        model_id: row.get(0)?,
                        customization_id: row.get(1)?,
                    },
                    names_json: "{}".to_owned(),
                    model_version: None,
                })
            })
            .and_then(Iterator::collect)
            .tx()
    }

    pub(super) fn open_series(&mut self, seed: &Seed<'_>) -> Result<BatchId, TxError> {
        let found: Option<i64> = self
            .tx
            .prepare_cached(sql::OPEN_BATCH_BY_KEY)
            .and_then(|mut statement| {
                statement
                    .query_row(params![self.account(), seed.key], |row| row.get(0))
                    .optional()
            })
            .tx()?;
        if let Some(id) = found {
            return Ok(BatchId(id));
        }
        let report_settings: String = self
            .tx
            .query_row(
                "SELECT report_settings FROM watched_folder WHERE id = ?1",
                [seed.folder.0],
                |row| row.get(0),
            )
            .tx()?;
        let batch = NewBatch {
            origin: Origin::Folder(seed.folder),
            batch_key: seed.key.to_owned(),
            label: seed.label.to_owned(),
            opened_at: seed.opened_at,
            report_settings_json: report_settings,
            app_version: seed.app_version.to_owned(),
            model_choices: self.folder_choices(seed.folder)?,
        };
        BatchRepo::open(self, &batch)
    }

    pub(super) fn cancel_work(&mut self, id: SourceFileId, now: Timestamp) -> Result<(), TxError> {
        let ids: Vec<i64> = self.column(sql::OPEN_COMPUTATIONS_OF_FILE, [id.0])?;
        for computation in ids {
            ComputationRepo::apply(
                self,
                ComputationId(computation),
                ComputationEvent::Cancel,
                now,
            )?;
        }
        self.tx
            .execute(sql::RELEASE_LEDGER_OF_FILE, [id.0])
            .tx()
            .map(drop)
    }

    pub(super) fn detect_file(&mut self, detection: &Detection) -> Result<Detected, TxError> {
        let batch_id = self.open_series(&Seed {
            folder: detection.folder,
            key: &detection.key.key,
            label: &detection.series_label,
            opened_at: detection.activity,
            app_version: &detection.app_version,
        })?;
        let id = SourceFileRepo::add(
            self,
            &NewSourceFile {
                batch_id,
                path: detection.path.clone(),
                label: detection.label.clone(),
                size: 0,
                mtime: detection.activity,
                next_attempt_at: detection.next_attempt_at,
            },
        )?;
        self.touch_batch(batch_id.0, detection.activity)?;
        let files: u32 = self
            .tx
            .query_row(
                "SELECT files FROM batch WHERE id = ?1",
                [batch_id.0],
                |row| row.get(0),
            )
            .tx()?;
        Ok(Detected {
            id,
            batch: batch_id,
            files,
        })
    }

    pub(super) fn remove_file(&mut self, id: SourceFileId, now: Timestamp) -> Result<(), TxError> {
        SourceFileRepo::apply(self, id, SourceFileEvent::Removed, now)?;
        self.cancel_work(id, now)?;
        let batch = self.batch_of(id)?;
        self.touch_batch(batch, now)
    }

    pub(super) fn change_file(&mut self, id: SourceFileId, now: Timestamp) -> Result<(), TxError> {
        let state = SourceFileRepo::apply(self, id, SourceFileEvent::ContentChanged, now)?;
        if state != SourceFileState::Detected {
            return Err(TxError::Conflict);
        }
        let batch = self.batch_of(id)?;
        self.touch_batch(batch, now)
    }
}

use mivia_domain::ids::{BatchId, ComputationId, Fingerprint, SourceFileId, Timestamp};
use mivia_ports::store::TxError;
use mivia_ports::store::pipeline::{AttemptRow, Fairness, PollRow, SubmitRow, UploadRow};
use rusqlite::{Row, params};

use super::SqlTx;
use crate::codec::parse_path;
use crate::errors::SqlResult;
use crate::sql::pipeline as sql;

pub(super) fn count(value: i64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn fairness(row: &Row<'_>) -> rusqlite::Result<Fairness> {
    Ok(Fairness {
        batch: BatchId(row.get(1)?),
        one_off: row.get(2)?,
        opened_at: Timestamp(row.get(3)?),
        arrival_seq: row.get(4)?,
    })
}

pub(super) type RawUpload = (i64, Fairness, String, String, i64, Vec<u8>, i64);

pub(super) fn raw_upload(row: &Row<'_>) -> rusqlite::Result<RawUpload> {
    Ok((
        row.get(0)?,
        fairness(row)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
    ))
}

pub(super) fn upload_row(raw: RawUpload) -> Result<UploadRow, TxError> {
    let (id, fairness, path, root, size, print, attempts) = raw;
    let fingerprint = <[u8; 32]>::try_from(print.as_slice())
        .map_err(|_| TxError::Corrupt("fingerprint".to_owned()))?;
    Ok(UploadRow {
        id: SourceFileId(id),
        fairness,
        path: parse_path(&path)?,
        root: parse_path(&root)?,
        size: u64::try_from(size).unwrap_or_default(),
        fingerprint: Fingerprint(fingerprint),
        attempts: count(attempts),
    })
}

pub(super) fn submit_row(row: &Row<'_>) -> rusqlite::Result<SubmitRow> {
    Ok(SubmitRow {
        id: ComputationId(row.get(0)?),
        fairness: fairness(row)?,
        choice_key: row.get(5)?,
        image_id: row.get(6)?,
        attempts: count(row.get(7)?),
    })
}

pub(super) fn poll_row(row: &Row<'_>) -> rusqlite::Result<PollRow> {
    Ok(PollRow {
        id: ComputationId(row.get(0)?),
        job_id: row.get(1)?,
        submitted_at: Timestamp(row.get(2)?),
        resubmitted: row.get(3)?,
    })
}

impl SqlTx<'_> {
    pub(super) fn rows<T>(
        &self,
        query: &str,
        values: impl rusqlite::Params,
        map: fn(&Row<'_>) -> rusqlite::Result<T>,
    ) -> Result<Vec<T>, TxError> {
        let mut statement = self.tx.prepare_cached(query).tx()?;
        statement
            .query_map(values, map)
            .and_then(Iterator::collect)
            .tx()
    }

    pub(super) fn attempt(&self, key: String) -> Result<AttemptRow, TxError> {
        let (choice_key, started_at, server_offset_ms) = self
            .tx
            .prepare_cached(sql::ATTEMPT)
            .and_then(|mut statement| {
                statement.query_row(params![key, self.account()], |row| {
                    Ok((row.get(0)?, Timestamp(row.get(1)?), row.get(2)?))
                })
            })
            .tx()?;
        let rows = self.rows(sql::ATTEMPT_MEMBERS, [&key], |row| {
            Ok((ComputationId(row.get(0)?), row.get(1)?, row.get(2)?))
        })?;
        let attempts = rows
            .iter()
            .map(|(_, _, n)| count(*n))
            .max()
            .unwrap_or_default();
        Ok(AttemptRow {
            key,
            choice_key,
            started_at,
            server_offset_ms,
            members: rows.into_iter().map(|(id, image, _)| (id, image)).collect(),
            attempts,
        })
    }
}

pub(super) fn serde_like_list(keys: &[String]) -> String {
    let quoted: Vec<String> = keys.iter().map(|key| format!("\"{key}\"")).collect();
    format!("[{}]", quoted.join(","))
}

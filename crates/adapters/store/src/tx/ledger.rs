use mivia_domain::ids::{FolderId, Timestamp};
use mivia_domain::path::WinPath;
use mivia_ports::fs::WalkEntry;
use mivia_ports::store::TxError;
use mivia_ports::store::intake::KnownPath;
use rusqlite::{Row, params};

use super::SqlTx;
use crate::codec::parse_path;
use crate::errors::SqlResult;
use crate::sql;

const BASELINE: [u8; 32] = [0; 32];

pub(super) struct RawKnown {
    path: String,
    size: i64,
    mtime: i64,
    fingerprint: Vec<u8>,
    given_up: bool,
}

impl RawKnown {
    pub(super) fn decode(self) -> Result<KnownPath, TxError> {
        Ok(KnownPath {
            path: parse_path(&self.path)?,
            size: u64::try_from(self.size).unwrap_or_default(),
            mtime: Timestamp(self.mtime),
            baseline: self.fingerprint == BASELINE,
            given_up: self.given_up,
        })
    }
}

pub(super) fn known_path(row: &Row<'_>) -> rusqlite::Result<RawKnown> {
    Ok(RawKnown {
        path: row.get(0)?,
        size: row.get(1)?,
        mtime: row.get(2)?,
        fingerprint: row.get(3)?,
        given_up: row.get(4)?,
    })
}

impl SqlTx<'_> {
    pub(super) fn known_paths_of(&self, folder: FolderId) -> Result<Vec<KnownPath>, TxError> {
        let mut statement = self.tx.prepare_cached(sql::KNOWN_PATHS).tx()?;
        let raws = statement
            .query_map([folder.0, self.account()], known_path)
            .and_then(Iterator::collect::<rusqlite::Result<Vec<_>>>)
            .tx()?;
        raws.into_iter().map(RawKnown::decode).collect()
    }

    pub(super) fn insert_baseline(
        &self,
        folder: FolderId,
        files: &[WalkEntry],
        now: Timestamp,
    ) -> Result<(), TxError> {
        let mut upsert = self.tx.prepare_cached(sql::UPSERT_LEDGER_PATH).tx()?;
        for file in files {
            upsert
                .execute(params![
                    folder.0,
                    file.path.to_string(),
                    BASELINE.as_slice(),
                    i64::try_from(file.size).unwrap_or(i64::MAX),
                    file.mtime.0,
                    now.0
                ])
                .tx()?;
        }
        Ok(())
    }

    pub(super) fn drain_baseline(
        &self,
        folder: FolderId,
        since: Timestamp,
    ) -> Result<Vec<WinPath>, TxError> {
        let values = params![folder.0, BASELINE.as_slice(), since.0];
        let texts: Vec<String> = self.column(sql::BASELINE_PATHS, values)?;
        self.tx
            .execute(
                "DELETE FROM ledger_path WHERE folder_id = ?1 AND fingerprint = ?2 \
                 AND seen_at >= ?3",
                values,
            )
            .tx()?;
        texts.iter().map(|text| parse_path(text)).collect()
    }
}

use mivia_domain::ids::{FolderId, Timestamp};
use mivia_domain::output::ProbeResult;
use mivia_domain::path::WinPath;
use mivia_ports::ipc::change::Change;
use mivia_ports::store::intake::{FolderChange, FolderRow};
use mivia_ports::store::{FolderRepo, NewFolder, TxError};
use rusqlite::{Row, params};

use super::SqlTx;
use crate::codec::{
    label_rule, parse_label_rule, parse_path, report_folder, report_folder_columns,
};
use crate::errors::SqlResult;
use crate::sql;

const INSERT_FOLDER: &str = "INSERT INTO watched_folder (account_id, path, path_key, position, \
     quiet_period, label_rule, report_settings, report_folder_mode, report_folder_name, \
     report_folder_path) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) RETURNING id";

const INSERT_CHOICE: &str = "INSERT INTO folder_model_choice (folder_id, model_choice_key, \
     model_id, customization_id, position) VALUES (?1, ?2, ?3, ?4, ?5)";

type RawFolder = (
    (i64, String, bool, i64, String, Option<i64>),
    (String, Option<String>, Option<String>, Option<String>),
);

fn raw_folder(row: &Row<'_>) -> rusqlite::Result<RawFolder> {
    Ok((
        (
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
        ),
        (row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?),
    ))
}

fn folder_row(raw: RawFolder) -> Result<FolderRow, TxError> {
    let ((id, path, active, quiet_ms, rule, missing), (mode, name, custom, probe)) = raw;
    Ok(FolderRow {
        id: FolderId(id),
        path: parse_path(&path)?,
        active,
        quiet_ms,
        label_rule: parse_label_rule(&rule)?,
        missing_since: missing.map(Timestamp),
        report_folder: report_folder(&mode, name, custom.as_deref())?,
        probe: probe.as_deref().and_then(ProbeResult::from_code),
    })
}

impl SqlTx<'_> {
    fn insert_folder_choices(&self, folder: i64, add: &NewFolder) -> Result<(), TxError> {
        let mut insert = self.tx.prepare_cached(INSERT_CHOICE).tx()?;
        for (position, choice) in add.model_choices.iter().enumerate() {
            insert
                .execute(params![
                    folder,
                    choice.key(),
                    choice.model_id,
                    choice.customization_id,
                    i64::try_from(position).unwrap_or(i64::MAX)
                ])
                .tx()?;
        }
        Ok(())
    }

    fn rebase_folder(&self, id: FolderId, to: &WinPath) -> Result<(), TxError> {
        let from = self.get_folder(id)?.path.to_string();
        let to = to.to_string();
        let length = i64::try_from(from.len()).unwrap_or(i64::MAX);
        self.tx
            .execute(
                "UPDATE ledger_path SET path = ?2 || substr(path, ?3 + 1) WHERE folder_id = ?1",
                params![id.0, to, length],
            )
            .tx()?;
        self.tx
            .execute(
                "UPDATE source_file SET path = ?2 || substr(path, ?3 + 1) \
                 WHERE batch_id IN (SELECT id FROM batch WHERE folder_id = ?1) \
                 AND substr(path, 1, ?3) = ?4",
                params![id.0, to, length, from],
            )
            .tx()?;
        Ok(())
    }

    fn set_probe(&mut self, id: FolderId, result: ProbeResult) -> Result<(), TxError> {
        self.tx
            .execute(
                "UPDATE watched_folder SET probe_result = ?3 WHERE id = ?1 AND account_id = ?2",
                params![id.0, self.account(), result.code()],
            )
            .tx()?;
        self.record(Change::Folder { id: id.to_string() });
        Ok(())
    }

    fn get_folder(&self, id: FolderId) -> Result<FolderRow, TxError> {
        let raw = self
            .tx
            .prepare_cached(&sql::folder_by_id())
            .and_then(|mut statement| {
                statement.query_row(params![self.account(), id.0], raw_folder)
            })
            .tx()?;
        folder_row(raw)
    }
}

impl FolderRepo for SqlTx<'_> {
    fn add(&mut self, folder: &NewFolder) -> Result<FolderId, TxError> {
        let (mode, name, custom) = report_folder_columns(&folder.report_folder);
        let id: i64 = self
            .tx
            .prepare_cached(INSERT_FOLDER)
            .and_then(|mut statement| {
                statement.query_row(
                    params![
                        self.account(),
                        folder.path.to_string(),
                        folder.path.key(),
                        folder.position,
                        folder.quiet_period_ms,
                        label_rule(folder.label_rule),
                        folder.report_settings_json,
                        mode,
                        name,
                        custom
                    ],
                    |row| row.get(0),
                )
            })
            .tx()?;
        self.insert_folder_choices(id, folder)?;
        self.record(Change::Folder { id: id.to_string() });
        Ok(FolderId(id))
    }

    fn get(&mut self, id: FolderId) -> Result<FolderRow, TxError> {
        self.get_folder(id)
    }

    fn list(&mut self) -> Result<Vec<FolderRow>, TxError> {
        let raws = self
            .tx
            .prepare_cached(&sql::folder_rows())
            .and_then(|mut statement| {
                statement
                    .query_map([self.account()], raw_folder)?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .tx()?;
        raws.into_iter().map(folder_row).collect()
    }

    fn every_path(&mut self) -> Result<Vec<WinPath>, TxError> {
        let texts: Vec<String> = self.column(
            "SELECT path FROM watched_folder WHERE removed_at IS NULL",
            [],
        )?;
        texts.iter().map(|text| parse_path(text)).collect()
    }

    fn update(&mut self, id: FolderId, change: &FolderChange) -> Result<(), TxError> {
        let (set, value): (&str, Option<i64>) = match change {
            FolderChange::Probe(result) => return self.set_probe(id, *result),
            FolderChange::Active(active) => ("active = ?3", Some(i64::from(*active))),
            FolderChange::Missing(since) => ("missing_since = ?3", since.map(|at| at.0)),
            FolderChange::Removed(at) => ("removed_at = ?3, active = 0", Some(at.0)),
            FolderChange::Relocated(path) => {
                self.rebase_folder(id, path)?;
                self.tx
                    .execute(
                        "UPDATE watched_folder SET path = ?3, path_key = ?4, missing_since = NULL \
                         WHERE id = ?1 AND account_id = ?2",
                        params![id.0, self.account(), path.to_string(), path.key()],
                    )
                    .tx()?;
                self.record(Change::Folder { id: id.to_string() });
                return Ok(());
            }
        };
        let changed = self
            .tx
            .execute(
                &format!("UPDATE watched_folder SET {set} WHERE id = ?1 AND account_id = ?2"),
                params![id.0, self.account(), value],
            )
            .tx()?;
        if changed == 0 {
            return Err(TxError::NotFound);
        }
        self.record(Change::Folder { id: id.to_string() });
        Ok(())
    }
}

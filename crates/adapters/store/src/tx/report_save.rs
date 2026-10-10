use mivia_domain::ids::{ComputationId, Fingerprint, ReportId, ReportPartId, Timestamp};
use mivia_domain::output::{ProbeResult, ReportFolder};
use mivia_domain::path::WinPath;
use mivia_domain::report::{PartEvent, PartState, SAVE_RETRY_MS, SaveError};
use mivia_ports::fs::Staged;
use mivia_ports::store::TxError;
use mivia_ports::store::reports::{SaveRow, SavedFile};
use rusqlite::{Row, params};

use super::SqlTx;
use super::report_parts::{format_of, language_of};
use crate::codec::{parse_path, report_folder};
use crate::errors::SqlResult;
use crate::sql::reports as sql;

type Text = Option<String>;

struct RawSave {
    format: String,
    part: (i64, i64),
    language: String,
    label: String,
    closed_at: i64,
    names: (Text, Text, String, Text),
    variant: bool,
    folder: (Text, Text, Text, Text, Text),
    first_member: Text,
    staged: (Text, Option<i64>, Option<Vec<u8>>, Text),
    one_off: Text,
}

fn raw_save(row: &Row<'_>) -> rusqlite::Result<RawSave> {
    Ok(RawSave {
        format: row.get(0)?,
        part: (row.get(1)?, row.get(2)?),
        language: row.get(3)?,
        label: row.get(4)?,
        closed_at: row.get(5)?,
        names: (row.get(6)?, row.get(7)?, row.get(8)?, row.get(20)?),
        variant: row.get::<_, Option<bool>>(9)?.unwrap_or_default(),
        folder: (
            row.get(10)?,
            row.get(11)?,
            row.get(12)?,
            row.get(13)?,
            row.get(14)?,
        ),
        first_member: row.get(15)?,
        staged: (row.get(16)?, row.get(17)?, row.get(18)?, row.get(19)?),
        one_off: row.get(21)?,
    })
}

fn missing(what: &str) -> TxError {
    TxError::Corrupt(format!("report part without {what}"))
}

fn optional_path(text: Option<&str>) -> Result<Option<WinPath>, TxError> {
    text.map(parse_path).transpose()
}

fn names_json(name: Text, id: &str) -> String {
    name.unwrap_or_else(|| format!("{{\"en\":{id:?},\"de\":{id:?}}}"))
}

fn one_off_home(target: &str) -> Result<(WinPath, ReportFolder), TxError> {
    let target = parse_path(target)?;
    Ok(match (target.parent(), target.file_name()) {
        (Some(parent), Some(name)) => (parent, ReportFolder::Subfolder(name.to_owned())),
        _ => (target, ReportFolder::BesideSeries),
    })
}

fn home(raw: &RawSave) -> Result<(Option<WinPath>, Option<ReportFolder>), TxError> {
    let (root, mode, name, custom, _) = &raw.folder;
    if let (None, Some(target)) = (root, &raw.one_off) {
        let (root, folder) = one_off_home(target)?;
        return Ok((Some(root), Some(folder)));
    }
    let folder = mode
        .as_deref()
        .map(|mode| report_folder(mode, name.clone(), custom.as_deref()))
        .transpose()?;
    Ok((optional_path(root.as_deref())?, folder))
}

fn save_row(id: ReportPartId, raw: RawSave) -> Result<SaveRow, TxError> {
    let (root, report_folder) = home(&raw)?;
    let (model_names, custom_names, model_id, custom_id) = raw.names;
    let probe = raw.folder.4;
    let (staging, size, sha, intended) = raw.staged;
    let sha: [u8; 32] = sha
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| missing("hash"))?;
    let variant_json = custom_id
        .filter(|_| raw.variant)
        .map(|custom| names_json(custom_names, &custom));
    Ok(SaveRow {
        id,
        format: format_of(&raw.format)?,
        part: (
            u32::try_from(raw.part.0).unwrap_or_default(),
            u32::try_from(raw.part.1).unwrap_or_default(),
        ),
        language: language_of(&raw.language)?,
        label: raw.label,
        closed_at: Timestamp(raw.closed_at),
        analysis_json: names_json(model_names, &model_id),
        variant_json,
        root,
        report_folder,
        probe: probe.as_deref().and_then(ProbeResult::from_code),
        first_member: optional_path(raw.first_member.as_deref())?,
        staging: parse_path(&staging.ok_or_else(|| missing("staging"))?)?,
        size: size.and_then(|n| u64::try_from(n).ok()).unwrap_or_default(),
        sha256: Fingerprint(sha),
        intended: optional_path(intended.as_deref())?,
    })
}

impl SqlTx<'_> {
    pub(super) fn record_download(
        &self,
        id: ReportPartId,
        staging: &WinPath,
        staged: &Staged,
    ) -> Result<(), TxError> {
        self.run(
            "UPDATE report_part SET staging_path = ?2, size = ?3, sha256 = ?4, \
             next_attempt_at = 0 WHERE id = ?1",
            params![
                id.0,
                staging.to_string(),
                i64::try_from(staged.size).unwrap_or(i64::MAX),
                staged.sha256.0.as_slice()
            ],
        )
    }

    pub(super) fn saves_due(&self, now: Timestamp) -> Result<Vec<SaveRow>, TxError> {
        let ids: Vec<i64> = self.column(sql::SAVE_DUE, params![self.account(), now.0])?;
        ids.into_iter()
            .map(|id| self.read_save_row(ReportPartId(id)))
            .collect()
    }

    pub(super) fn read_save_row(&self, id: ReportPartId) -> Result<SaveRow, TxError> {
        let raw = self
            .tx
            .prepare_cached(sql::SAVE_ROW)
            .and_then(|mut statement| statement.query_row([id.0, self.account()], raw_save))
            .tx()?;
        save_row(id, raw)
    }

    pub(super) fn first_save(&self) -> Result<Option<Timestamp>, TxError> {
        let next: Vec<Option<i64>> = self.column(sql::NEXT_SAVE, [self.account()])?;
        Ok(next.into_iter().flatten().next().map(Timestamp))
    }

    pub(super) fn record_saved(
        &mut self,
        id: ReportPartId,
        path: &WinPath,
        now: Timestamp,
    ) -> Result<(), TxError> {
        self.part_event(id, PartEvent::Saved, now)?;
        self.run(
            "UPDATE report_part SET file_path = ?2, staging_path = NULL, save_problem = NULL \
             WHERE id = ?1",
            params![id.0, path.to_string()],
        )
    }

    pub(super) fn record_save_failure(
        &mut self,
        id: ReportPartId,
        error: SaveError,
        now: Timestamp,
    ) -> Result<PartState, TxError> {
        let problem = match error {
            SaveError::Retryable(problem) => Some(problem.code()),
            SaveError::InvalidName | SaveError::TooManyClashes => None,
        };
        let after = self.part_event(id, PartEvent::SaveFailed(error), now)?;
        self.run(
            "UPDATE report_part SET save_problem = ?2, next_attempt_at = ?3 WHERE id = ?1",
            params![id.0, problem, now.plus_ms(SAVE_RETRY_MS).0],
        )?;
        Ok(after)
    }

    pub(super) fn recover_report_parts(&self, now: Timestamp) -> Result<(), TxError> {
        self.run(sql::RECOVER_POLLS, params![self.account(), now.0])?;
        self.run(sql::RECOVER_SAVES, params![self.account(), now.0])
    }

    pub(super) fn read_saved_file(&self, id: ReportPartId) -> Result<Option<SavedFile>, TxError> {
        let rows: Vec<(String, i64, Vec<u8>)> = self.rows(
            "SELECT p.file_path, p.size, p.sha256 FROM report_part p \
             JOIN report r ON r.id = p.report_id WHERE p.id = ?1 AND r.account_id = ?2 \
             AND p.state = 'saved' AND p.file_path IS NOT NULL AND p.sha256 IS NOT NULL",
            [id.0, self.account()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        rows.into_iter()
            .next()
            .map(|(path, size, sha)| {
                let sha: [u8; 32] = sha.try_into().map_err(|_| missing("hash"))?;
                Ok(SavedFile {
                    path: parse_path(&path)?,
                    size: u64::try_from(size).unwrap_or_default(),
                    sha256: Fingerprint(sha),
                })
            })
            .transpose()
    }

    pub(super) fn stale_after_run_again(&mut self, id: ComputationId) -> Result<(), TxError> {
        let reports: Vec<i64> = self.column(sql::STALE_CANDIDATES, [id.0])?;
        reports
            .into_iter()
            .try_for_each(|report| self.mark_stale(ReportId(report), true))
    }
}

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use mivia_domain::ids::Timestamp;
use rusqlite::{Connection, OpenFlags};

use crate::open::{self, OpenError};

const KEEP_BEFORE_MIGRATION: usize = 2;
const KEEP_DAILY: usize = 7;
const KEEP_MONTHLY: usize = 6;
const DAILY_PREFIX: &str = "mivia-";
const DAILY_SUFFIX: &str = ".db";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RestoreError {
    Unreadable(String),
    Corrupt(Vec<String>),
    UnknownSchema(i64),
    SchemaMismatch,
    Io(String),
}

fn sibling(db_path: &Path, suffix: &str) -> PathBuf {
    let mut name = db_path.file_name().map(OsString::from).unwrap_or_default();
    name.push(suffix);
    db_path.with_file_name(name)
}

pub(crate) fn vacuum_into(conn: &Connection, target: &Path) -> rusqlite::Result<()> {
    conn.execute("VACUUM INTO ?1", [target.to_string_lossy()])
        .map(drop)
}

pub(crate) fn before_migration(
    conn: &Connection,
    db_path: &Path,
    from: i64,
) -> Result<(), OpenError> {
    let backup_error = |error: String| OpenError::Backup(error);
    let target = sibling(db_path, &format!(".bak-{from}"));
    if target.exists() {
        fs::remove_file(&target).map_err(|error| backup_error(error.to_string()))?;
    }
    vacuum_into(conn, &target).map_err(|error| backup_error(error.to_string()))?;
    prune_before_migration(db_path).map_err(|error| backup_error(error.to_string()))
}

fn prune_before_migration(db_path: &Path) -> std::io::Result<()> {
    let prefix = format!(
        "{}.bak-",
        db_path.file_name().unwrap_or_default().to_string_lossy()
    );
    let mut versions = list_names(db_path.parent().unwrap_or_else(|| Path::new(".")))?
        .into_iter()
        .filter_map(|name| name.strip_prefix(&prefix)?.parse::<i64>().ok())
        .collect::<Vec<_>>();
    versions.sort_unstable_by(|a, b| b.cmp(a));
    for version in versions.into_iter().skip(KEEP_BEFORE_MIGRATION) {
        fs::remove_file(sibling(db_path, &format!(".bak-{version}")))?;
    }
    Ok(())
}

fn list_names(dir: &Path) -> std::io::Result<Vec<String>> {
    fs::read_dir(dir)?
        .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
        .collect()
}

pub fn daily_backup_name(date: &str) -> String {
    format!("{DAILY_PREFIX}{date}{DAILY_SUFFIX}")
}

pub fn expired_daily_backups(dates: &[String]) -> Vec<String> {
    let mut sorted = dates.to_vec();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    sorted.dedup();
    let mut keep = sorted
        .iter()
        .take(KEEP_DAILY)
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut months = BTreeSet::new();
    for date in sorted.iter().rev() {
        months.insert(date.split('-').take(2).collect::<Vec<_>>().join("-"));
    }
    for month in months.iter().rev().take(KEEP_MONTHLY) {
        if let Some(first) = sorted
            .iter()
            .rev()
            .find(|date| date.starts_with(month.as_str()))
        {
            keep.insert(first.clone());
        }
    }
    sorted
        .into_iter()
        .filter(|date| !keep.contains(date))
        .collect()
}

pub fn prune_daily(dir: &Path) -> std::io::Result<()> {
    let dates = list_names(dir)?
        .into_iter()
        .filter_map(|name| {
            let date = name
                .strip_prefix(DAILY_PREFIX)?
                .strip_suffix(DAILY_SUFFIX)?;
            (date.len() == 10).then(|| date.to_owned())
        })
        .collect::<Vec<_>>();
    for date in expired_daily_backups(&dates) {
        fs::remove_file(dir.join(daily_backup_name(&date)))?;
    }
    Ok(())
}

pub fn verify_backup(candidate: &Path) -> Result<i64, RestoreError> {
    let unreadable = |error: rusqlite::Error| RestoreError::Unreadable(error.to_string());
    let conn = Connection::open_with_flags(candidate, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(unreadable)?;
    crate::config::harden(&conn).map_err(unreadable)?;
    let problems = integrity_problems(&conn, "integrity_check").map_err(unreadable)?;
    if !problems.is_empty() {
        return Err(RestoreError::Corrupt(problems));
    }
    let major: i64 = conn
        .query_row("SELECT schema_major FROM meta", [], |row| row.get(0))
        .map_err(unreadable)?;
    if major != open::SCHEMA_MAJOR {
        return Err(RestoreError::UnknownSchema(major));
    }
    let reference = open::migrate_in_memory().map_err(RestoreError::Unreadable)?;
    let expected = schema_rows(&reference).map_err(unreadable)?;
    if schema_rows(&conn).map_err(unreadable)? != expected {
        return Err(RestoreError::SchemaMismatch);
    }
    Ok(major)
}

pub fn restore_backup(
    candidate: &Path,
    db_path: &Path,
    now: Timestamp,
) -> Result<(), RestoreError> {
    verify_backup(candidate)?;
    let io = |error: std::io::Error| RestoreError::Io(error.to_string());
    let staged = sibling(db_path, ".restore-tmp");
    fs::copy(candidate, &staged).map_err(io)?;
    set_aside(db_path, &format!(".before-restore-{}", now.0)).map_err(io)?;
    fs::rename(&staged, db_path).map_err(io)
}

pub fn backup_series(candidate: &Path) -> Option<u32> {
    let conn = Connection::open_with_flags(candidate, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    crate::config::harden(&conn).ok()?;
    conn.query_row("SELECT count(*) FROM batch", [], |row| row.get(0))
        .ok()
}

pub(crate) fn restore_in_place(
    conn: &mut Connection,
    candidate: &Path,
    db_path: &Path,
    now: Timestamp,
) -> Result<(), RestoreError> {
    verify_backup(candidate)?;
    let failed = |error: rusqlite::Error| RestoreError::Io(error.to_string());
    vacuum_into(
        conn,
        &sibling(db_path, &format!(".before-restore-{}", now.0)),
    )
    .map_err(failed)?;
    conn.restore(
        rusqlite::MAIN_DB,
        candidate,
        None::<fn(rusqlite::backup::Progress)>,
    )
    .map_err(failed)
}

pub fn quarantine_corrupt(db_path: &Path, now: Timestamp) -> std::io::Result<PathBuf> {
    let suffix = format!(".corrupt-{}", now.0);
    set_aside(db_path, &suffix)?;
    Ok(sibling(db_path, &suffix))
}

fn set_aside(db_path: &Path, suffix: &str) -> std::io::Result<()> {
    for companion in ["", "-wal", "-shm"] {
        let from = sibling(db_path, companion);
        if from.exists() {
            fs::rename(&from, sibling(db_path, &format!("{suffix}{companion}")))?;
        }
    }
    Ok(())
}

pub(crate) fn integrity_problems(conn: &Connection, pragma: &str) -> rusqlite::Result<Vec<String>> {
    let mut statement = conn.prepare(&format!("PRAGMA {pragma}"))?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().filter(|row| row != "ok").collect())
}

type SchemaRow = (String, String, Option<String>);

pub(crate) fn schema_rows(conn: &Connection) -> rusqlite::Result<Vec<SchemaRow>> {
    let mut statement =
        conn.prepare("SELECT type, name, sql FROM sqlite_schema ORDER BY type, name")?;
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .collect()
}

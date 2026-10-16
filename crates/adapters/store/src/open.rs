use std::path::Path;

use rusqlite::{Connection, OptionalExtension};
use rusqlite_migration::{M, Migrations};

use crate::backup;

pub const SCHEMA_MAJOR: i64 = 1;

const MIGRATION_SQL: &[&str] = &[
    include_str!("../migrations/0001_init.sql"),
    include_str!("../migrations/0002_telemetry.sql"),
    include_str!("../migrations/0003_watch_marker.sql"),
    include_str!("../migrations/0004_history_delete.sql"),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenError {
    NewerSchema { min_reader_major: i64 },
    Backup(String),
    Migration(String),
    Sqlite(String),
}

impl From<rusqlite::Error> for OpenError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error.to_string())
    }
}

pub(crate) fn migrations() -> Migrations<'static> {
    Migrations::new(MIGRATION_SQL.iter().map(|sql| M::up(sql)).collect())
}

fn latest_version() -> i64 {
    i64::try_from(MIGRATION_SQL.len()).unwrap_or(i64::MAX)
}

pub(crate) fn prepare(conn: &mut Connection, db_path: &Path) -> Result<(), OpenError> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > latest_version() {
        return accept_newer(conn);
    }
    if version > 0 && version < latest_version() {
        backup::before_migration(conn, db_path, version)?;
    }
    migrations()
        .to_latest(conn)
        .map_err(|error| OpenError::Migration(error.to_string()))
}

fn accept_newer(conn: &Connection) -> Result<(), OpenError> {
    let min_reader_major: i64 = conn
        .query_row("SELECT min_reader_major FROM meta", [], |row| row.get(0))
        .optional()?
        .unwrap_or(i64::MAX);
    if min_reader_major > SCHEMA_MAJOR {
        return Err(OpenError::NewerSchema { min_reader_major });
    }
    Ok(())
}

pub(crate) fn migrate_in_memory() -> Result<Connection, String> {
    let mut conn = Connection::open_in_memory().map_err(|error| error.to_string())?;
    crate::config::harden(&conn).map_err(|error| error.to_string())?;
    migrations()
        .to_latest(&mut conn)
        .map_err(|error| error.to_string())?;
    Ok(conn)
}

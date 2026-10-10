use std::time::Duration;

use rusqlite::Connection;
use rusqlite::config::DbConfig;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoreConfig {
    pub synchronous_full: bool,
}

impl StoreConfig {
    pub const fn production() -> Self {
        Self {
            synchronous_full: true,
        }
    }

    pub const fn for_tests() -> Self {
        Self {
            synchronous_full: false,
        }
    }
}

const BUSY_TIMEOUT: Duration = Duration::from_millis(5_000);

const WRITER_PRAGMAS: &str = "PRAGMA auto_vacuum = INCREMENTAL;
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
PRAGMA cache_size = -2000;
PRAGMA mmap_size = 0;
PRAGMA journal_size_limit = 8388608;
PRAGMA wal_autocheckpoint = 1000;";

const READER_PRAGMAS: &str = "PRAGMA query_only = ON;
PRAGMA cache_size = -1000;
PRAGMA mmap_size = 0;";

pub(crate) fn harden(conn: &Connection) -> rusqlite::Result<()> {
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)?;
    conn.execute_batch("PRAGMA trusted_schema = OFF; PRAGMA cell_size_check = ON;")?;
    conn.busy_timeout(BUSY_TIMEOUT)
}

pub(crate) fn configure_writer(conn: &Connection, config: StoreConfig) -> rusqlite::Result<()> {
    harden(conn)?;
    conn.execute_batch(WRITER_PRAGMAS)?;
    let synchronous = if config.synchronous_full {
        "FULL"
    } else {
        "OFF"
    };
    conn.pragma_update(None, "synchronous", synchronous)?;
    conn.set_prepared_statement_cache_capacity(32);
    Ok(())
}

pub(crate) fn configure_reader(conn: &Connection) -> rusqlite::Result<()> {
    harden(conn)?;
    conn.execute_batch(READER_PRAGMAS)
}

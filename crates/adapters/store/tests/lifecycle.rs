use std::sync::Arc;
use std::sync::atomic::Ordering;

use mivia_domain::ids::{AccountId, Timestamp};
use mivia_ports::ipc::history::HistoryFilter;
use mivia_ports::queries::Queries;
use mivia_ports::store::{AccountScope, StoreError, StoreExt, TxError};
use mivia_store::{
    OpenError, RestoreError, SqliteStore, StoreConfig, quarantine_corrupt, restore_backup,
    verify_backup,
};
use rusqlite::Connection;

use crate::common::{Recorder, account, closed_batch, db_path, fixture};

fn pragma(conn: &Connection, name: &str) -> String {
    conn.query_row(&format!("PRAGMA {name}"), [], |row| {
        row.get::<_, rusqlite::types::Value>(0)
    })
    .map(|value| format!("{value:?}"))
    .unwrap()
}

#[tokio::test]
async fn schema_matches_snapshot_and_file_settings() {
    let fx = fixture(StoreConfig::production()).await;
    let conn = Connection::open(fx.db()).unwrap();
    let mut statement = conn
        .prepare("SELECT type, name, sql FROM sqlite_schema ORDER BY type, name")
        .unwrap();
    let schema: Vec<String> = statement
        .query_map([], |row| {
            let sql: Option<String> = row.get(2)?;
            Ok(format!(
                "{} {}\n{}\n",
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                sql.unwrap_or_default()
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    insta::assert_snapshot!("schema", schema.join("\n"));
    assert_eq!(pragma(&conn, "auto_vacuum"), "Integer(2)");
    assert_eq!(pragma(&conn, "journal_mode"), r#"Text("wal")"#);
    assert_eq!(pragma(&conn, "user_version"), "Integer(5)");
    let options: Vec<String> = conn
        .prepare("PRAGMA compile_options")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    for flag in [
        "DEFAULT_MEMSTATUS=0",
        "DQS=0",
        "OMIT_DEPRECATED",
        "OMIT_SHARED_CACHE",
        "ENABLE_FTS5",
    ] {
        assert!(
            options.iter().any(|option| option == flag),
            "{flag} missing in {options:?}"
        );
    }
}

#[tokio::test]
async fn newer_schema_opens_only_when_readable() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let path = fx.db();
    drop(fx.store);
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("PRAGMA user_version = 6; UPDATE meta SET min_reader_major = 1;")
        .unwrap();
    let observer = Arc::new(Recorder::default());
    assert!(
        SqliteStore::open(&path, StoreConfig::for_tests(), Arc::clone(&observer) as _)
            .await
            .is_ok()
    );
    conn.execute_batch("UPDATE meta SET min_reader_major = 2;")
        .unwrap();
    let refused = SqliteStore::open(&path, StoreConfig::for_tests(), observer).await;
    assert_eq!(
        refused.err(),
        Some(OpenError::NewerSchema {
            min_reader_major: 2
        })
    );
}

#[tokio::test]
async fn failed_and_panicking_writes_roll_back() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let failed = fx
        .store
        .write(|tx| {
            account(tx, "a");
            Err::<(), _>(TxError::Conflict)
        })
        .await;
    assert_eq!(failed, Err(StoreError::Tx(TxError::Conflict)));
    let panicked = fx
        .store
        .write(|tx| -> Result<(), TxError> {
            account(tx, "b");
            std::panic::panic_any("boom")
        })
        .await;
    assert_eq!(panicked, Err(StoreError::Panicked));
    let scope = fx.store.write(|tx| Ok(account(tx, "c"))).await.unwrap();
    assert_eq!(scope, AccountScope(AccountId(1)));
    let conn = Connection::open(fx.db()).unwrap();
    let count: i64 = conn
        .query_row("SELECT count(*) FROM account", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn revision_moves_only_with_changes_and_observer_sees_it() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let scope = fx.store.write(|tx| Ok(account(tx, "a"))).await.unwrap();
    assert_eq!(fx.recorder.revision.load(Ordering::SeqCst), 0);
    fx.store
        .write(move |tx| Ok(closed_batch(tx, scope, "S1")))
        .await
        .unwrap();
    assert_eq!(fx.recorder.revision.load(Ordering::SeqCst), 1);
    assert_eq!(fx.recorder.commits.load(Ordering::SeqCst), 2);
    let page = fx
        .store
        .list_history(scope, HistoryFilter::default(), None)
        .await
        .unwrap();
    assert_eq!(page.revision, "1");
}

#[tokio::test]
async fn readers_are_kept_only_on_request() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let scope = fx.store.write(|tx| Ok(account(tx, "a"))).await.unwrap();
    fx.store.list_folders(scope).await.unwrap();
    assert_eq!(fx.store.open_readers().await, 0);
    fx.store.keep_readers();
    fx.store.list_folders(scope).await.unwrap();
    assert_eq!(fx.store.open_readers().await, 1);
    fx.store.list_folders(scope).await.unwrap();
    assert_eq!(fx.store.open_readers().await, 2);
    fx.store.close_readers().await;
    assert_eq!(fx.store.open_readers().await, 0);
    fx.store.list_folders(scope).await.unwrap();
    assert_eq!(fx.store.open_readers().await, 0);
}

#[tokio::test]
async fn history_events_are_insert_only() {
    let fx = fixture(StoreConfig::for_tests()).await;
    fx.store.write(|tx| Ok(account(tx, "a"))).await.unwrap();
    let conn = Connection::open(fx.db()).unwrap();
    conn.execute(
        "INSERT INTO history_event (at, tz, windows_user, account_id, action, trigger) \
         VALUES (1, 'UTC', 'lab', 1, 'series_opened', 'folder')",
        [],
    )
    .unwrap();
    assert!(conn.execute("UPDATE history_event SET at = 2", []).is_err());
    assert!(conn.execute("DELETE FROM history_event", []).is_err());
}

#[tokio::test]
async fn backups_verify_restore_and_quarantine() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let scope = fx.store.write(|tx| Ok(account(tx, "a"))).await.unwrap();
    fx.store
        .write(move |tx| Ok(closed_batch(tx, scope, "Probe 1")))
        .await
        .unwrap();
    assert_eq!(fx.store.quick_check().await.unwrap(), Vec::<String>::new());
    let backups = fx.dir.path().join("backups");
    std::fs::create_dir(&backups).unwrap();
    fx.store
        .daily_backup(backups.clone(), Timestamp(1_791_594_123_456))
        .await
        .unwrap();
    let backup = backups.join("mivia-2026-10-10.db");
    assert_eq!(verify_backup(&backup), Ok(1));

    let tampered = fx.dir.path().join("tampered.db");
    std::fs::copy(&backup, &tampered).unwrap();
    Connection::open(&tampered)
        .unwrap()
        .execute_batch("CREATE TRIGGER evil AFTER INSERT ON account BEGIN SELECT 1; END;")
        .unwrap();
    assert_eq!(verify_backup(&tampered), Err(RestoreError::SchemaMismatch));
    let junk = fx.dir.path().join("junk.db");
    std::fs::write(&junk, b"not a database at all, just text").unwrap();
    assert!(matches!(
        verify_backup(&junk),
        Err(RestoreError::Unreadable(_))
    ));

    let target_dir = tempfile::tempdir().unwrap();
    let target = db_path(target_dir.path());
    std::fs::write(&target, b"old").unwrap();
    restore_backup(&backup, &target, Timestamp(7)).unwrap();
    assert_eq!(verify_backup(&target), Ok(1));
    assert!(target_dir.path().join("mivia.db.before-restore-7").exists());
    let moved = quarantine_corrupt(&target, Timestamp(8)).unwrap();
    assert!(moved.ends_with("mivia.db.corrupt-8"));
    assert!(!target.exists());
}

#[tokio::test]
async fn search_index_rebuilds() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let scope = fx.store.write(|tx| Ok(account(tx, "a"))).await.unwrap();
    fx.store
        .write(move |tx| Ok(closed_batch(tx, scope, "Stahlprobe")))
        .await
        .unwrap();
    fx.store.rebuild_search().await.unwrap();
    let conn = Connection::open(fx.db()).unwrap();
    let hits: i64 = conn
        .query_row(
            "SELECT count(*) FROM batch_search WHERE batch_search MATCH '\"hlpro\"'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(hits, 1);
}

#[test]
fn daily_retention_keeps_seven_days_and_six_months() {
    let mut dates = Vec::new();
    for month in 1..=9 {
        for day in [1, 15] {
            dates.push(format!("2026-{month:02}-{day:02}"));
        }
    }
    for day in 20..=30 {
        dates.push(format!("2026-09-{day:02}"));
    }
    let mut expired = mivia_store::expired_daily_backups(&dates);
    expired.sort();
    let mut kept: Vec<String> = dates.into_iter().filter(|d| !expired.contains(d)).collect();
    kept.sort();
    assert_eq!(
        kept,
        [
            "2026-04-01",
            "2026-05-01",
            "2026-06-01",
            "2026-07-01",
            "2026-08-01",
            "2026-09-01",
            "2026-09-24",
            "2026-09-25",
            "2026-09-26",
            "2026-09-27",
            "2026-09-28",
            "2026-09-29",
            "2026-09-30",
        ]
    );
}

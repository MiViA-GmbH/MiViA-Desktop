use std::fmt::Write as _;

use mivia_domain::hold::{Access, Gate, HoldReason, can_claim};
use rusqlite::Connection;

use crate::codec::{HOLD_REASONS, hold_reason};
use crate::open::migrate_in_memory;
use crate::sql;

const GATES: [Gate; 7] = [
    Gate::Upload,
    Gate::Submit,
    Gate::StatusPoll,
    Gate::ReportRequest,
    Gate::ReportDownload,
    Gate::Catalogue,
    Gate::Labels,
];

fn plan(conn: &Connection, query: &str) -> Vec<String> {
    let mut statement = conn
        .prepare(&format!("EXPLAIN QUERY PLAN {query}"))
        .unwrap();
    let nulls = std::iter::repeat_n(rusqlite::types::Null, statement.parameter_count());
    statement
        .query_map(rusqlite::params_from_iter(nulls), |row| {
            row.get::<_, String>(3)
        })
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

const FORBIDDEN: &[&str] = &["USE TEMP B-TREE", "MULTI-INDEX OR", "AUTOMATIC"];

fn forbidden(line: &str) -> bool {
    let scans_table = line.starts_with("SCAN ") && !line.starts_with("SCAN json_each ");
    scans_table || FORBIDDEN.iter().any(|pattern| line.contains(pattern))
}

#[test]
fn claim_and_list_plans_use_indexes() {
    let conn = migrate_in_memory().unwrap();
    let mut report = String::new();
    for (name, query) in sql::planned() {
        let lines = plan(&conn, &query);
        for line in &lines {
            assert!(!forbidden(line), "{name}: {line}");
        }
        writeln!(report, "{name}:").unwrap();
        for line in lines {
            writeln!(report, "  {line}").unwrap();
        }
    }
    insta::assert_snapshot!("query_plans", report);
}

fn sql_access(conn: &Connection, gate: Gate) -> Access {
    let level: i64 = conn
        .query_row(&sql::gate_access(gate), [1], |row| row.get(0))
        .unwrap();
    match level {
        0 => Access::Allowed,
        1 => Access::Throttled {
            min_interval_ms: mivia_domain::hold::SERVICE_UNAVAILABLE_POLL_MS,
        },
        _ => Access::Blocked,
    }
}

#[test]
fn sql_gate_predicate_equals_domain_matrix_for_every_hold_subset() {
    let conn = migrate_in_memory().unwrap();
    conn.execute(
        "INSERT INTO account (id, server_url, server_account_id, display_name) VALUES (1, 'u', 'a', 'A')",
        [],
    )
    .unwrap();
    for mask in 0_u32..(1 << HOLD_REASONS.len()) {
        let holds: Vec<HoldReason> = HOLD_REASONS
            .iter()
            .enumerate()
            .filter(|(bit, _)| mask & (1 << bit) != 0)
            .map(|(_, reason)| *reason)
            .collect();
        conn.execute("DELETE FROM account_hold", []).unwrap();
        for reason in &holds {
            conn.execute(
                "INSERT INTO account_hold (account_id, reason, since) VALUES (1, ?1, 0)",
                [hold_reason(*reason)],
            )
            .unwrap();
        }
        for gate in GATES {
            assert_eq!(
                sql_access(&conn, gate),
                can_claim(gate, &holds),
                "{gate:?} with {holds:?}"
            );
        }
    }
}

#[test]
fn accounts_from_before_issued_keys_have_no_issued_key_after_the_migration() {
    let mut conn = Connection::open_in_memory().unwrap();
    let migrations = crate::open::migrations();
    migrations.to_version(&mut conn, 4).unwrap();
    conn.execute(
        "INSERT INTO account (id, server_url, server_account_id, display_name) VALUES (1, 'u', 'a', 'A')",
        [],
    )
    .unwrap();
    migrations.to_latest(&mut conn).unwrap();
    let stored: Option<String> = conn
        .query_row(
            "SELECT issued_key_id FROM account WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored, None);
}

#[test]
fn migrations_are_valid() {
    crate::open::migrations().validate().unwrap();
}

#[test]
fn pre_migration_backups_keep_the_two_newest() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("mivia.db");
    let conn = Connection::open(&db).unwrap();
    conn.execute_batch("CREATE TABLE t (x INTEGER)").unwrap();
    for from in 1..=3 {
        crate::backup::before_migration(&conn, &db, from).unwrap();
    }
    let mut names: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".bak-"))
        .collect();
    names.sort();
    assert_eq!(names, ["mivia.db.bak-2", "mivia.db.bak-3"]);
}

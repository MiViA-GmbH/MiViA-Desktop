use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use mivia_domain::history::{fts_phrase, search_term};
use mivia_domain::natural::sort_key;
use mivia_ports::ipc::history::{HistoryFilter, HistorySort, HistoryStatus};
use rusqlite::types::Value;
use rusqlite::{Connection, StatementStatus, params};

use crate::sql::history::{Plan, window};

const BATCHES: i64 = 100_000;
const OPEN: i64 = 5;
const FOLDERS: i64 = 9;
const FIXTURE_VERSION: u32 = 2;
const MATERIALS: [&str; 6] = [
    "42CrMo4",
    "S355",
    "GJS-400",
    "Härte",
    "Gefüge",
    "Einschlüsse",
];
const PREFIXES: [&str; 5] = ["img", "probe", "Schliff", "Härte", "sample"];

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../target/testdata")
        .join(format!("history-1m-v{FIXTURE_VERSION}.db"))
}

const fn members(batch: i64) -> i64 {
    1 + (batch * 7919) % 19
}

fn insert_batches(conn: &Connection) -> rusqlite::Result<()> {
    let mut batch = conn.prepare(
        "INSERT INTO batch (id, account_id, origin, folder_id, batch_key, label, label_sort, open, \
         opened_at, closed_at, sort_at, report_settings, files, problems, active_count, \
         reports_problem, reports_stale, needs_attention, app_version) \
         VALUES (?1, 1, ?2, ?3, ?1, ?4, ?5, ?6, ?7, ?8, ?9, '{}', ?10, ?11, ?12, ?13, ?14, ?15, '2.0.0')",
    )?;
    let mut choice = conn.prepare(
        "INSERT INTO batch_model_choice (batch_id, account_id, model_choice_key, model_id, \
         names, sort_at) VALUES (?1, 1, ?2 || ':', ?2, '{}', ?3)",
    )?;
    for id in 1..=BATCHES {
        let open = id > BATCHES - OPEN;
        let one_off = id % 10 == 0;
        let label = format!(
            "Probe {id} {}",
            MATERIALS[usize::try_from(id % 6).unwrap_or_default()]
        );
        let at = 1_700_000_000_000 + id * 60_000;
        let sort_at = if open { i64::MAX } else { at };
        let flags = (id % 33 == 0, id % 97 == 0, id % 71 == 0, id % 50 == 0);
        batch.execute(params![
            id,
            if one_off { "one_off" } else { "folder" },
            (!one_off).then_some(1 + id % FOLDERS),
            label,
            sort_key(&label),
            open,
            at - 600_000,
            (!open).then_some(at),
            sort_at,
            members(id),
            i64::from(flags.0),
            i64::from(flags.1),
            i64::from(flags.2),
            i64::from(flags.3),
            flags.0 || flags.2 || flags.3
        ])?;
        for model in [id % 5, (id + 1) % 5] {
            choice.execute(params![id, format!("m{model}"), sort_at])?;
        }
    }
    Ok(())
}

fn insert_files(conn: &Connection) -> rusqlite::Result<()> {
    let mut file = conn.prepare(
        "INSERT INTO source_file (account_id, batch_id, path, label, size, mtime, state, \
         arrival_seq) VALUES (1, ?1, ?2, ?3, 1000, 0, 'uploaded', ?4)",
    )?;
    for id in 1..=BATCHES {
        for seq in 1..=members(id) {
            let prefix = PREFIXES[usize::try_from((id + seq) % 5).unwrap_or_default()];
            let label = format!("{prefix}_{seq:04}.tif");
            file.execute(params![id, format!("D:\\Lab\\{id}\\{label}"), label, seq])?;
        }
    }
    Ok(())
}

fn build(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let partial = path.with_extension("partial");
    drop(std::fs::remove_file(&partial));
    let mut conn = Connection::open(&partial).unwrap();
    conn.execute_batch("PRAGMA journal_mode = OFF; PRAGMA synchronous = OFF;")
        .unwrap();
    crate::open::migrations().to_latest(&mut conn).unwrap();
    let tx = conn.transaction().unwrap();
    tx.execute_batch(
        "INSERT INTO account (id, server_url, server_account_id, display_name) \
         VALUES (1, 'https://app.mivia.ai', 'a', 'Lab'), (2, 'https://app.mivia.ai', 'b', 'Other');",
    )
    .unwrap();
    for folder in 1..=FOLDERS {
        tx.execute(
            "INSERT INTO watched_folder (id, account_id, path, path_key, position, quiet_period, \
             label_rule, report_settings) VALUES (?1, 1, ?2, ?2, ?1, 60000, 'auto', '{}')",
            params![folder, format!("D:\\Lab\\F{folder}")],
        )
        .unwrap();
    }
    insert_batches(&tx).unwrap();
    insert_files(&tx).unwrap();
    tx.execute_batch(
        "INSERT INTO batch_search (rowid, text) SELECT b.sort_at, b.label || char(10) || \
         (SELECT group_concat(label, char(10)) FROM source_file WHERE batch_id = b.id) \
         FROM batch b WHERE b.open = 0;",
    )
    .unwrap();
    tx.commit().unwrap();
    drop(conn);
    std::fs::rename(&partial, path).unwrap();
}

fn filter(sort: HistorySort, ascending: bool) -> HistoryFilter {
    HistoryFilter {
        sort,
        ascending,
        ..HistoryFilter::default()
    }
}

fn combos() -> Vec<(String, HistoryFilter)> {
    let mut out = Vec::new();
    for (sort, sort_name) in [
        (HistorySort::Finished, "finished"),
        (HistorySort::Name, "name"),
    ] {
        for (ascending, dir) in [(false, "desc"), (true, "asc")] {
            let base = filter(sort, ascending);
            let variants: Vec<(&str, HistoryFilter)> = vec![
                ("all", base.clone()),
                (
                    "folder",
                    HistoryFilter {
                        folder: Some("3".to_owned()),
                        ..base.clone()
                    },
                ),
                (
                    "manual",
                    HistoryFilter {
                        manual: true,
                        ..base.clone()
                    },
                ),
                (
                    "analysis",
                    HistoryFilter {
                        analysis: Some("m2".to_owned()),
                        ..base.clone()
                    },
                ),
                (
                    "problems",
                    HistoryFilter {
                        status: Some(HistoryStatus::Problems),
                        ..base.clone()
                    },
                ),
                (
                    "out-of-date",
                    HistoryFilter {
                        status: Some(HistoryStatus::OutOfDate),
                        ..base.clone()
                    },
                ),
                (
                    "active",
                    HistoryFilter {
                        status: Some(HistoryStatus::Active),
                        ..base.clone()
                    },
                ),
                (
                    "date",
                    HistoryFilter {
                        from: Some("2023-11-20T00:00:00.000Z".to_owned()),
                        to: Some("2023-11-27T00:00:00.000Z".to_owned()),
                        ..base.clone()
                    },
                ),
                (
                    "folder+problems",
                    HistoryFilter {
                        folder: Some("5".to_owned()),
                        status: Some(HistoryStatus::Problems),
                        ..base.clone()
                    },
                ),
                (
                    "folder+analysis",
                    HistoryFilter {
                        folder: Some("5".to_owned()),
                        analysis: Some("m4".to_owned()),
                        ..base.clone()
                    },
                ),
                (
                    "search common",
                    HistoryFilter {
                        q: Some("img_0003".to_owned()),
                        ..base.clone()
                    },
                ),
                (
                    "search diacritics",
                    HistoryFilter {
                        q: Some("harte".to_owned()),
                        ..base.clone()
                    },
                ),
                (
                    "search rare",
                    HistoryFilter {
                        q: Some("Probe 9876 ".to_owned()),
                        ..base.clone()
                    },
                ),
                (
                    "search none",
                    HistoryFilter {
                        q: Some("zzzq".to_owned()),
                        ..base.clone()
                    },
                ),
                (
                    "search+folder",
                    HistoryFilter {
                        q: Some("sample".to_owned()),
                        folder: Some("2".to_owned()),
                        ..base.clone()
                    },
                ),
                (
                    "search+manual+problems",
                    HistoryFilter {
                        q: Some("schliff".to_owned()),
                        manual: true,
                        status: Some(HistoryStatus::Problems),
                        ..base.clone()
                    },
                ),
            ];
            for (name, filter) in variants {
                out.push((format!("{sort_name} {dir} {name}"), filter));
            }
        }
    }
    out
}

fn plan_lines(conn: &Connection, sql: &str, binds: &[(&str, &dyn rusqlite::ToSql)]) -> Vec<String> {
    let mut statement = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
    statement
        .query_map(binds, |row| row.get::<_, String>(3))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

fn forbidden(line: &str) -> bool {
    let scan = line.starts_with("SCAN ") && !line.contains("VIRTUAL TABLE INDEX");
    let bad = ["USE TEMP B-TREE", "MULTI-INDEX OR", "AUTOMATIC"];
    scan || bad.iter().any(|pattern| line.contains(pattern))
}

struct Run {
    rows: usize,
    vm_steps: i32,
    last: Option<(Value, i64)>,
}

fn run(conn: &Connection, name: &str, plan: &Plan<'_>, report: &mut String) -> Run {
    let window = window(plan);
    let binds: Vec<(&str, &dyn rusqlite::ToSql)> = window
        .binds
        .iter()
        .map(|(key, value)| (*key, value.as_ref()))
        .collect();
    let lines = plan_lines(conn, &window.sql, &binds);
    for line in &lines {
        assert!(
            !forbidden(line),
            "{name}: {line}
{}
{lines:#?}",
            window.sql
        );
    }
    let mut statement = conn.prepare(&window.sql).unwrap();
    let rows: Vec<(i64, Value)> = statement
        .query_map(binds.as_slice(), |row| Ok((row.get(0)?, row.get(9)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    let counters = [
        ("sort", StatementStatus::Sort),
        ("auto index", StatementStatus::AutoIndex),
        ("full scan", StatementStatus::FullscanStep),
    ];
    for (counter, status) in counters {
        assert_eq!(statement.get_status(status), 0, "{name}: {counter}");
    }
    let vm_steps = statement.get_status(StatementStatus::VmStep);
    writeln!(report, "{name}: rows {} vm {vm_steps}", rows.len()).unwrap();
    Run {
        rows: rows.len(),
        vm_steps,
        last: rows.last().map(|(id, key)| (key.clone(), *id)),
    }
}

fn ceiling(filter: &HistoryFilter) -> i32 {
    match (filter.sort, &filter.q) {
        (HistorySort::Name, Some(_)) => 2_000_000,
        _ => 200_000,
    }
}

fn matrix(conn: &Connection, stats: &str) -> String {
    let mut report = String::new();
    for (name, filter) in combos() {
        let term = filter.q.as_deref().and_then(search_term);
        let range = filter
            .from
            .as_ref()
            .map(|_| (1_701_500_000_000, 1_702_000_000_000));
        let mut plan = Plan {
            account: 1,
            filter: &filter,
            folder: filter.folder.as_deref().and_then(|id| id.parse().ok()),
            range,
            fts: term.as_ref().map(fts_phrase),
            after: None,
        };
        let ceiling = ceiling(&filter);
        let first = run(conn, &format!("{stats} {name} first"), &plan, &mut report);
        assert!(first.vm_steps < ceiling, "{name}: {} steps", first.vm_steps);
        if first.rows > 0 {
            plan.after = first.last;
            let next = run(conn, &format!("{stats} {name} next"), &plan, &mut report);
            assert!(next.vm_steps < ceiling, "{name}: {} steps", next.vm_steps);
        }
    }
    report
}

#[test]
fn history_page_queries_stay_on_their_indexes_at_one_million_files() {
    let path = fixture_path();
    if !path.exists() {
        build(&path);
    }
    let mut conn = Connection::open(&path).unwrap();
    let count: i64 = conn
        .query_row("SELECT count(*) FROM source_file", [], |row| row.get(0))
        .unwrap();
    assert!(count > 900_000, "{count} source files");
    let mut report = matrix(&conn, "no-stats");
    let tx = conn.transaction().unwrap();
    tx.execute_batch("ANALYZE").unwrap();
    report.push_str(&matrix(&tx, "analyzed"));
    tx.rollback().unwrap();
    insta::assert_snapshot!("history_1m_gate", report);
}

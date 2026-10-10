use mivia_domain::history::SCAN_BUDGET;
use mivia_ports::ipc::history::{HistoryFilter, HistorySort, HistoryStatus};
use rusqlite::ToSql;
use rusqlite::types::Value;

use super::PAGE_SIZE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Driver {
    Time,
    Folder,
    Manual,
    Analysis,
    Problems,
    Search,
    Label,
    FolderLabel,
}

pub(crate) struct Window {
    pub(crate) sql: String,
    pub(crate) binds: Vec<(&'static str, Box<dyn ToSql>)>,
    pub(crate) limit: i64,
}

pub(crate) struct Plan<'a> {
    pub(crate) account: i64,
    pub(crate) filter: &'a HistoryFilter,
    pub(crate) folder: Option<i64>,
    pub(crate) range: Option<(i64, i64)>,
    pub(crate) fts: Option<String>,
    pub(crate) after: Option<(Value, i64)>,
}

pub(crate) const fn is_label(driver: Driver) -> bool {
    matches!(driver, Driver::Label | Driver::FolderLabel)
}

fn start(driver: Driver, ascending: bool) -> (Value, i64) {
    match (is_label(driver), ascending) {
        (true, true) => (Value::Blob(Vec::new()), i64::MIN),
        (true, false) => (Value::Blob(vec![u8::MAX; 2]), i64::MAX),
        (false, true) => (Value::Integer(i64::MIN), i64::MIN),
        (false, false) => (Value::Integer(i64::MAX), i64::MAX),
    }
}

pub(crate) fn driver(plan: &Plan<'_>) -> Driver {
    let filter = plan.filter;
    match (filter.sort, plan.folder, plan.fts.is_some()) {
        (HistorySort::Name, Some(_), _) => Driver::FolderLabel,
        (HistorySort::Name, None, _) => Driver::Label,
        (HistorySort::Finished, _, true) => Driver::Search,
        (HistorySort::Finished, Some(_), false) => Driver::Folder,
        (HistorySort::Finished, None, false) if filter.manual => Driver::Manual,
        (HistorySort::Finished, None, false) if filter.analysis.is_some() => Driver::Analysis,
        (HistorySort::Finished, None, false) => match filter.status {
            Some(HistoryStatus::Problems | HistoryStatus::OutOfDate) => Driver::Problems,
            Some(HistoryStatus::Active) | None => Driver::Time,
        },
    }
}

const fn serves_date(driver: Driver) -> bool {
    !is_label(driver)
}

pub(crate) fn needs_budget(plan: &Plan<'_>, driver: Driver) -> bool {
    let filter = plan.filter;
    let served_folder = matches!(driver, Driver::Folder | Driver::FolderLabel);
    let extra_folder = plan.folder.is_some() && !served_folder;
    let extra_manual = filter.manual && driver != Driver::Manual;
    let extra_model = filter.analysis.is_some() && driver != Driver::Analysis;
    let extra_search = plan.fts.is_some() && driver != Driver::Search;
    let extra_date = plan.range.is_some() && !serves_date(driver);
    let checks = [
        extra_folder,
        extra_manual,
        extra_model,
        extra_search,
        extra_date,
        filter.status.is_some(),
        driver == Driver::Search,
    ];
    checks.into_iter().any(|check| check)
}

const COLUMNS: &str = "b.id, b.folder_id, b.origin = 'one_off', b.problems, b.reports_problem, \
     b.reports_stale, b.active_count, b.sort_at, b.account_id";

fn batch(index: &str, extra: &str, key: &'static str) -> (String, String, &'static str) {
    (
        format!("batch b INDEXED BY {index}"),
        format!("b.account_id = :acc{extra} AND b.open = 0"),
        key,
    )
}

const IN_FOLDER: &str = " AND b.folder_id = :folder";

fn source(driver: Driver) -> (String, String, &'static str) {
    match driver {
        Driver::Time => batch("batch_hist_time", "", "b.sort_at"),
        Driver::Folder => batch("batch_hist_folder", IN_FOLDER, "b.sort_at"),
        Driver::Manual => batch(
            "batch_hist_manual",
            " AND b.origin = 'one_off'",
            "b.sort_at",
        ),
        Driver::Problems => batch(
            "batch_hist_problems",
            " AND (problems > 0 OR reports_problem > 0 OR reports_stale > 0)",
            "b.sort_at",
        ),
        Driver::Label => batch("batch_hist_label", "", "b.label_sort"),
        Driver::FolderLabel => batch("batch_hist_folder_label", IN_FOLDER, "b.label_sort"),
        Driver::Analysis => (
            "batch_model_choice m INDEXED BY bmc_hist CROSS JOIN batch b ON b.id = m.batch_id"
                .to_owned(),
            "m.account_id = :acc AND m.model_id = :model AND b.open = 0".to_owned(),
            "m.sort_at",
        ),
        Driver::Search => (
            "batch_search s CROSS JOIN batch b INDEXED BY batch_closed_sort \
             ON b.sort_at = s.rowid AND b.open = 0"
                .to_owned(),
            "batch_search MATCH :q".to_owned(),
            "s.rowid",
        ),
    }
}

fn tie(driver: Driver) -> Option<&'static str> {
    match driver {
        Driver::Search => None,
        Driver::Analysis => Some("m.batch_id"),
        Driver::Time
        | Driver::Folder
        | Driver::Manual
        | Driver::Problems
        | Driver::Label
        | Driver::FolderLabel => Some("b.id"),
    }
}

fn keyset(key: &str, tie: Option<&str>, ascending: bool) -> (String, String) {
    let (cmp, dir) = if ascending {
        (">", "ASC")
    } else {
        ("<", "DESC")
    };
    tie.map_or_else(
        || (format!("{key} {cmp} :k"), format!("{key} {dir}")),
        |tie| {
            (
                format!("({key}, {tie}) {cmp} (:k, :id)"),
                format!("{key} {dir}, {tie} {dir}"),
            )
        },
    )
}

fn extra_columns(plan: &Plan<'_>, driver: Driver) -> String {
    let model = match (&plan.filter.analysis, driver) {
        (Some(_), Driver::Analysis) | (None, _) => "1".to_owned(),
        (Some(_), _) => "EXISTS (SELECT 1 FROM batch_model_choice x \
             WHERE x.batch_id = b.id AND x.model_id = :model)"
            .to_owned(),
    };
    let search = match (&plan.fts, driver) {
        (Some(_), Driver::Search) | (None, _) => "1",
        (Some(_), _) => "b.sort_at IN (SELECT rowid FROM batch_search WHERE batch_search MATCH :q)",
    };
    format!("{model}, {search}")
}

fn date_range(plan: &Plan<'_>, driver: Driver, key: &str) -> String {
    match plan.range {
        Some(_) if serves_date(driver) => format!(" AND {key} BETWEEN :from AND :to"),
        Some(_) | None => String::new(),
    }
}

pub(crate) fn window(plan: &Plan<'_>) -> Window {
    let driver = driver(plan);
    let (from, base, key) = source(driver);
    let (range, order) = keyset(key, tie(driver), plan.filter.ascending);
    let limit = if needs_budget(plan, driver) {
        SCAN_BUDGET
    } else {
        PAGE_SIZE + 1
    };
    let sql = format!(
        "SELECT {COLUMNS}, {key}, {}, b.label_sort FROM {from} WHERE {base} AND {range}{} \
         ORDER BY {order} LIMIT {limit}",
        extra_columns(plan, driver),
        date_range(plan, driver, key),
    );
    Window {
        binds: binds(plan, driver, &sql),
        sql,
        limit,
    }
}

fn binds(plan: &Plan<'_>, driver: Driver, sql: &str) -> Vec<(&'static str, Box<dyn ToSql>)> {
    let mut binds: Vec<(&'static str, Box<dyn ToSql>)> = Vec::new();
    let mut add = |name: &'static str, value: Box<dyn ToSql>| {
        if sql.contains(name) {
            binds.push((name, value));
        }
    };
    let (key, id) = plan
        .after
        .clone()
        .unwrap_or_else(|| start(driver, plan.filter.ascending));
    add(":acc", Box::new(plan.account));
    add(":k", Box::new(key));
    add(":id", Box::new(id));
    add(":folder", Box::new(plan.folder));
    add(":model", Box::new(plan.filter.analysis.clone()));
    add(":q", Box::new(plan.fts.clone()));
    add(":from", Box::new(plan.range.map(|range| range.0)));
    add(":to", Box::new(plan.range.map(|range| range.1)));
    binds
}

pub(crate) const REPORT_ID_MATCHES: &str = "SELECT r.batch_id FROM report_part p \
     INDEXED BY report_part_server_id JOIN report r ON r.id = p.report_id \
     WHERE p.server_report_id >= ?2 AND p.server_report_id < ?2 || 'g' \
     AND r.account_id = ?1 AND r.batch_id IS NOT NULL LIMIT 20";

pub(crate) const MEMBER_LABELS: &str = "SELECT label FROM source_file INDEXED BY source_file_by_batch \
     WHERE batch_id = ?1 ORDER BY arrival_seq";

pub(crate) const SAVED_FILES: &str = "SELECT p.file_path FROM report r INDEXED BY report_by_batch \
     JOIN report_part p ON p.report_id = r.id WHERE r.batch_id = ?1 AND p.state = 'saved' \
     AND p.file_path IS NOT NULL";

pub(crate) const CUSTOM_WINDOW: &str = "SELECT r.id, r.created_at, r.custom_name FROM report r \
     INDEXED BY report_custom WHERE r.account_id = ?1 AND r.kind = 'custom' \
     AND (r.created_at, r.id) < (?2, ?3) ORDER BY r.created_at DESC, r.id DESC LIMIT ?4";

pub(crate) const CUSTOM_ROW: &str = "SELECT r.id, r.custom_name, r.created_at, r.state, r.stale, \
     r.model_choice_key, \
     (SELECT count(*) FROM report_member rm WHERE rm.report_id = r.id AND rm.omitted IS NULL), \
     (SELECT c.batch_id FROM report_member rm JOIN computation c ON c.id = rm.computation_id \
     WHERE rm.report_id = r.id ORDER BY rm.seq LIMIT 1), \
     EXISTS (SELECT 1 FROM report_part p WHERE p.report_id = r.id \
     AND p.state IN ('not_saved', 'failed')), \
     COALESCE((SELECT p.id FROM report_part p WHERE p.report_id = r.id \
     AND p.state = 'saved' AND p.format = 'pdf' ORDER BY p.part_no LIMIT 1), \
     (SELECT p.id FROM report_part p WHERE p.report_id = r.id AND p.state = 'saved' \
     ORDER BY p.part_no LIMIT 1)) \
     FROM report r WHERE r.id = ?1";

pub(crate) const BATCH_EVENTS: &str = "SELECT e.id, e.at, e.tz, e.windows_user, e.action, \
     e.trigger, e.before_json, e.after_json FROM history_event e INDEXED BY history_event_batch \
     WHERE e.batch_id = ?2 AND e.id < ?3 AND e.account_id = ?1 ORDER BY e.id DESC LIMIT ?4";

pub(crate) const RENAMABLE: &str = "SELECT b.open = 1 OR NOT EXISTS (SELECT 1 FROM report r \
     INDEXED BY report_by_batch JOIN report_part p ON p.report_id = r.id \
     WHERE r.batch_id = b.id AND (p.requested_at IS NOT NULL OR p.server_report_id IS NOT NULL)) \
     FROM batch b WHERE b.id = ?1";

#[cfg(test)]
pub(crate) fn planned() -> Vec<(&'static str, String)> {
    vec![
        ("find series by report id", REPORT_ID_MATCHES.to_owned()),
        ("list member labels", MEMBER_LABELS.to_owned()),
        ("list saved report files", SAVED_FILES.to_owned()),
        ("list custom report window", CUSTOM_WINDOW.to_owned()),
        ("get custom report row", CUSTOM_ROW.to_owned()),
        ("list series events", BATCH_EVENTS.to_owned()),
        ("check series renamable", RENAMABLE.to_owned()),
    ]
}

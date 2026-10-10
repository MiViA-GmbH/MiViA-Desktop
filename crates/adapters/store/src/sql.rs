pub(crate) mod pipeline;
pub(crate) mod reports;
pub(crate) mod screens;

use mivia_domain::hold::Gate;

pub(crate) const CLAIM_LIMIT: i64 = 50;
pub(crate) const PAGE_SIZE: i64 = 100;

const BLOCKED: &str = "2";

fn hold_case(gate: Gate) -> &'static str {
    match gate {
        Gate::Upload => BLOCKED,
        Gate::Submit => {
            "CASE WHEN h.reason IN ('license_invalid.uploads', 'client_too_old') THEN 0 ELSE 2 END"
        }
        Gate::StatusPoll => {
            "CASE WHEN h.reason = 'service_unavailable' THEN 1 \
             WHEN h.reason GLOB 'license_invalid.*' OR h.reason = 'client_too_old' THEN 0 ELSE 2 END"
        }
        Gate::ReportRequest => {
            "CASE WHEN h.reason GLOB 'license_invalid.*' OR h.reason = 'client_too_old' THEN 0 ELSE 2 END"
        }
        Gate::ReportDownload | Gate::Catalogue => {
            "CASE WHEN h.reason IN ('service_unavailable', 'client_too_old') \
             OR h.reason GLOB 'license_invalid.*' THEN 0 ELSE 2 END"
        }
        Gate::Labels => {
            "CASE WHEN h.reason = 'offline' OR h.reason GLOB 'network_blocked.*' THEN 2 ELSE 0 END"
        }
    }
}

pub(crate) fn gate_access(gate: Gate) -> String {
    format!(
        "SELECT COALESCE(MAX({}), 0) FROM account_hold h WHERE h.account_id = ?1",
        hold_case(gate)
    )
}

fn gate_open(gate: Gate) -> String {
    format!("({}) < 2", gate_access(gate))
}

pub(crate) fn source_due_probe() -> String {
    "SELECT id FROM source_file WHERE account_id = ?1 AND state = 'detected' \
     AND next_attempt_at <= ?2 AND id NOT IN (SELECT value FROM json_each(?3)) \
     ORDER BY next_attempt_at, id LIMIT ?4"
        .to_owned()
}

pub(crate) fn source_due_upload() -> String {
    format!(
        "SELECT id FROM source_file WHERE account_id = ?1 AND state = 'ready' \
         AND next_attempt_at <= ?2 AND id NOT IN (SELECT value FROM json_each(?3)) AND {} \
         ORDER BY next_attempt_at, id LIMIT ?4",
        gate_open(Gate::Upload)
    )
}

pub(crate) fn computation_due_submit() -> String {
    format!(
        "SELECT id FROM computation WHERE state IN ('queued', 'submitting', 'computing') \
         AND state = 'queued' AND next_attempt_at <= ?2 AND account_id = ?1 \
         AND id NOT IN (SELECT value FROM json_each(?3)) AND {} \
         ORDER BY next_attempt_at, id LIMIT ?4",
        gate_open(Gate::Submit)
    )
}

pub(crate) fn computation_due_poll() -> String {
    format!(
        "SELECT id FROM computation WHERE state = 'computing' AND account_id = ?1 \
         AND next_poll_at <= ?2 AND id NOT IN (SELECT value FROM json_each(?3)) AND {} \
         ORDER BY next_poll_at, id LIMIT ?4",
        gate_open(Gate::StatusPoll)
    )
}

pub(crate) const HOLD_DUE_PROBE: &str = "SELECT reason FROM account_hold \
     WHERE account_id = ?1 AND next_probe_at <= ?2 ORDER BY next_probe_at";

pub(crate) const BATCH_OPEN: &str =
    "SELECT id FROM batch WHERE account_id = ?1 AND open = 1 ORDER BY id";

pub(crate) const BATCH_COLUMNS: &str = "id, label, origin, folder_id, open, opened_at, closed_at, \
     files, done, failed, problems, active_count, reports_saved, reports_problem, reports_total, \
     reports_stale, needs_attention, sort_at, closure_reason";

pub(crate) fn history_body() -> String {
    format!(
        "SELECT {BATCH_COLUMNS} FROM batch INDEXED BY batch_hist_time \
         WHERE account_id = ?1 AND open = 0 AND (sort_at, id) < (?2, ?3) \
         ORDER BY sort_at DESC, id DESC LIMIT {}",
        PAGE_SIZE + 1
    )
}

pub(crate) fn history_head() -> String {
    format!(
        "SELECT {BATCH_COLUMNS} FROM batch INDEXED BY batch_open \
         WHERE account_id = ?1 AND open = 1 ORDER BY id DESC"
    )
}

pub(crate) fn attention_series() -> String {
    format!(
        "SELECT {BATCH_COLUMNS} FROM batch INDEXED BY batch_attention \
         WHERE account_id = ?1 AND needs_attention = 1 ORDER BY sort_at DESC, id DESC LIMIT 6"
    )
}

pub(crate) const ATTENTION_COUNT: &str = "SELECT count(*) FROM batch INDEXED BY batch_attention \
     WHERE account_id = ?1 AND needs_attention = 1";

pub(crate) fn batch_by_id() -> String {
    format!("SELECT {BATCH_COLUMNS} FROM batch WHERE id = ?2 AND account_id = ?1")
}

pub(crate) fn batch_items() -> String {
    format!(
        "SELECT id, label, state, arrival_seq, reason FROM source_file \
         WHERE batch_id = ?2 AND account_id = ?1 AND arrival_seq > ?3 \
         ORDER BY arrival_seq LIMIT {}",
        PAGE_SIZE + 1
    )
}

pub(crate) const ITEM_COMPUTATIONS: &str = "SELECT id, model_choice_key, state, reason FROM computation \
     WHERE source_file_id = ?1 ORDER BY model_choice_key, id";

const FOLDER_COLUMNS: &str = "id, path, active, quiet_period, label_rule, missing_since, \
     report_folder_mode, report_folder_name, report_folder_path, probe_result";

pub(crate) fn folder_rows() -> String {
    format!(
        "SELECT {FOLDER_COLUMNS} FROM watched_folder \
         WHERE account_id = ?1 AND removed_at IS NULL ORDER BY position, id"
    )
}

pub(crate) fn folder_by_id() -> String {
    format!("SELECT {FOLDER_COLUMNS} FROM watched_folder WHERE account_id = ?1 AND id = ?2")
}

pub(crate) const FOLDER_CHOICES: &str = "SELECT model_id, customization_id \
     FROM folder_model_choice WHERE folder_id = ?1 ORDER BY position";

pub(crate) const LIVE_MEMBERS: &str = "SELECT s.id, s.batch_id, s.path, s.state, s.reason, \
     s.size, s.mtime FROM batch b INDEXED BY batch_open JOIN source_file s ON s.batch_id = b.id \
     WHERE b.account_id = ?1 AND b.open = 1 AND b.folder_id = ?2 \
     AND s.state NOT IN ('vanished', 'unreadable')";

pub(crate) const OPEN_BATCH_BY_KEY: &str =
    "SELECT id FROM batch WHERE account_id = ?1 AND batch_key = ?2 AND open = 1";

pub(crate) const OPEN_COMPUTATIONS_OF_FILE: &str = "SELECT id FROM computation \
     WHERE source_file_id = ?1 AND state IN ('queued', 'submitting', 'computing')";

pub(crate) const DONE_COMPUTATIONS_OF_FILE: &str =
    "SELECT id FROM computation WHERE source_file_id = ?1 AND state = 'done'";

pub(crate) const RELEASE_LEDGER_OF_FILE: &str = "DELETE FROM ledger_entry \
     WHERE (folder_id, fingerprint) IN (SELECT b.folder_id, s.fingerprint FROM source_file s \
     JOIN batch b ON b.id = s.batch_id WHERE s.id = ?1) \
     AND computation_id IN (SELECT id FROM computation WHERE source_file_id = ?1 \
     AND state = 'cancelled')";

pub(crate) const CLAIM_PROBES: &str = "SELECT s.id, f.path, s.path, s.attempts, s.first_busy_at \
     FROM source_file s JOIN batch b ON b.id = s.batch_id \
     JOIN watched_folder f ON f.id = b.folder_id \
     WHERE s.account_id = ?1 AND s.state = 'detected' AND s.next_attempt_at <= ?2 \
     AND s.id NOT IN (SELECT value FROM json_each(?3)) \
     ORDER BY s.next_attempt_at, s.id LIMIT ?4";

pub(crate) const NEXT_PROBE: &str = "SELECT min(next_attempt_at) FROM source_file \
     WHERE account_id = ?1 AND state = 'detected'";

pub(crate) const READY_ROW: &str = "SELECT s.batch_id, b.folder_id, s.path, s.fingerprint, \
     s.copy_of FROM source_file s JOIN batch b ON b.id = s.batch_id \
     WHERE s.id = ?1 AND s.account_id = ?2";

pub(crate) const STORE_READ: &str = "UPDATE source_file SET fingerprint = ?2, size = ?3, \
     mtime = ?4, change_time = ?5, attempts = 0, first_busy_at = NULL WHERE id = ?1";

pub(crate) const EARLIER_IN_BATCH: &str = "SELECT id FROM source_file \
     WHERE batch_id = ?1 AND fingerprint = ?2 AND id <> ?3 AND copy_of IS NULL \
     AND state IN ('ready', 'known_content', 'uploading', 'uploaded') \
     ORDER BY arrival_seq LIMIT 1";

pub(crate) const IN_LEDGER: &str =
    "SELECT 1 FROM ledger_entry WHERE folder_id = ?1 AND fingerprint = ?2 LIMIT 1";

pub(crate) const PATHS_WITH_CONTENT: &str = "SELECT path, size, mtime, fingerprint \
     FROM ledger_path INDEXED BY ledger_path_by_fingerprint \
     WHERE folder_id = ?1 AND fingerprint = ?2";

pub(crate) const UPSERT_LEDGER_PATH: &str = "INSERT INTO ledger_path \
     (folder_id, path, fingerprint, size, mtime, seen_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
     ON CONFLICT (folder_id, path) DO UPDATE SET fingerprint = excluded.fingerprint, \
     size = excluded.size, mtime = excluded.mtime, seen_at = excluded.seen_at";

pub(crate) const COMPUTATION_PLAN: &str = "SELECT s.batch_id, m.model_choice_key, \
     b.app_version, e.computation_id, ifnull(c.state = 'done', 0) FROM source_file s \
     JOIN batch b ON b.id = s.batch_id JOIN batch_model_choice m ON m.batch_id = s.batch_id \
     LEFT JOIN ledger_entry e ON e.folder_id = b.folder_id AND e.fingerprint = s.fingerprint \
     AND e.model_choice_key = m.model_choice_key \
     LEFT JOIN computation c ON c.id = e.computation_id \
     WHERE s.id = ?1 ORDER BY m.model_choice_key";

pub(crate) const CLAIM_LEDGER: &str = "INSERT INTO ledger_entry \
     (folder_id, fingerprint, model_choice_key, computation_id) \
     SELECT b.folder_id, s.fingerprint, ?2, ?3 FROM source_file s \
     JOIN batch b ON b.id = s.batch_id WHERE s.id = ?1 ON CONFLICT DO NOTHING";

pub(crate) const KNOWN_PATHS: &str =
    "SELECT path, size, mtime, fingerprint FROM ledger_path WHERE folder_id = ?1";

pub(crate) const BASELINE_PATHS: &str = "SELECT path FROM ledger_path \
     INDEXED BY ledger_path_by_fingerprint \
     WHERE folder_id = ?1 AND fingerprint = ?2 AND seen_at >= ?3 ORDER BY path";

pub(crate) const OPEN_BATCH_ROWS: &str = "SELECT b.id, b.folder_id, b.batch_key, b.opened_at, \
     ifnull(b.last_member_at, b.opened_at), b.files, \
     (SELECT count(*) FROM source_file s WHERE s.batch_id = b.id AND s.state = 'detected'), \
     f.quiet_period FROM batch b INDEXED BY batch_open \
     JOIN watched_folder f ON f.id = b.folder_id \
     WHERE b.account_id = ?1 AND b.open = 1 ORDER BY b.id";

pub(crate) const MEMBER_PATHS: &str = "SELECT path FROM source_file \
     WHERE batch_id = ?1 AND account_id = ?2 \
     AND state NOT IN ('vanished', 'unreadable', 'known_file') ORDER BY arrival_seq";

pub(crate) const DETECTED_IN_BATCH: &str =
    "SELECT id FROM source_file WHERE batch_id = ?1 AND state = 'detected' ORDER BY arrival_seq";

pub(crate) const MOVE_MEMBER: &str = "UPDATE source_file SET batch_id = ?2, \
     arrival_seq = COALESCE((SELECT max(arrival_seq) FROM source_file WHERE batch_id = ?2), 0) + 1 \
     WHERE id = ?1";

#[cfg(test)]
pub(crate) fn planned() -> Vec<(&'static str, String)> {
    let mut queries = vec![
        ("claim source probe", source_due_probe()),
        ("claim source upload", source_due_upload()),
        ("claim computation submit", computation_due_submit()),
        ("claim computation poll", computation_due_poll()),
        ("claim hold probe", HOLD_DUE_PROBE.to_owned()),
        ("claim open batches", BATCH_OPEN.to_owned()),
        ("list history body", history_body()),
        ("list history head", history_head()),
        ("list attention series", attention_series()),
        ("count attention series", ATTENTION_COUNT.to_owned()),
        ("get batch", batch_by_id()),
        ("list batch items", batch_items()),
        ("list item computations", ITEM_COMPUTATIONS.to_owned()),
        ("list folder rows", folder_rows()),
        ("list live members", LIVE_MEMBERS.to_owned()),
        ("find open batch by key", OPEN_BATCH_BY_KEY.to_owned()),
        ("claim readiness probes", CLAIM_PROBES.to_owned()),
        ("next readiness probe", NEXT_PROBE.to_owned()),
        ("find earlier copy in batch", EARLIER_IN_BATCH.to_owned()),
        ("find content in ledger", IN_LEDGER.to_owned()),
        ("list paths with content", PATHS_WITH_CONTENT.to_owned()),
        ("plan computations", COMPUTATION_PLAN.to_owned()),
        ("list known paths", KNOWN_PATHS.to_owned()),
        ("list baseline paths", BASELINE_PATHS.to_owned()),
        ("list open batch rows", OPEN_BATCH_ROWS.to_owned()),
        ("list member paths", MEMBER_PATHS.to_owned()),
        ("list detected members", DETECTED_IN_BATCH.to_owned()),
    ];
    queries.extend(pipeline::planned());
    queries.extend(reports::planned());
    queries.extend(screens::planned());
    queries
}

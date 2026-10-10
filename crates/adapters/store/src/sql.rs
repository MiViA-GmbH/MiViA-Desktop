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

pub(crate) fn part_due_request() -> String {
    format!(
        "SELECT p.id FROM report_part p JOIN report r ON r.id = p.report_id \
         WHERE p.state = 'requested' AND p.server_report_id IS NULL AND r.account_id = ?1 \
         AND p.id NOT IN (SELECT value FROM json_each(?3)) AND {} \
         ORDER BY p.id LIMIT ?4",
        gate_open(Gate::ReportRequest)
    )
}

pub(crate) fn part_due_poll() -> String {
    format!(
        "SELECT p.id FROM report_part p JOIN report r ON r.id = p.report_id \
         WHERE p.state = 'requested' AND p.server_report_id IS NOT NULL AND p.next_poll_at <= ?2 \
         AND r.account_id = ?1 AND p.id NOT IN (SELECT value FROM json_each(?3)) AND {} \
         ORDER BY p.next_poll_at, p.id LIMIT ?4",
        gate_open(Gate::ReportDownload)
    )
}

pub(crate) const HOLD_DUE_PROBE: &str = "SELECT reason FROM account_hold \
     WHERE account_id = ?1 AND next_probe_at <= ?2 ORDER BY next_probe_at";

pub(crate) const BATCH_OPEN: &str =
    "SELECT id FROM batch WHERE account_id = ?1 AND open = 1 ORDER BY id";

pub(crate) const BATCH_COLUMNS: &str = "id, label, origin, folder_id, open, opened_at, closed_at, \
     files, done, failed, problems, active_count, reports_saved, reports_problem, reports_total, \
     reports_stale, needs_attention, sort_at";

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
        "SELECT id, label, state, arrival_seq FROM source_file \
         WHERE batch_id = ?2 AND account_id = ?1 AND arrival_seq > ?3 \
         ORDER BY arrival_seq LIMIT {}",
        PAGE_SIZE + 1
    )
}

pub(crate) const ITEM_COMPUTATIONS: &str = "SELECT id, model_choice_key, state FROM computation \
     WHERE source_file_id = ?1 ORDER BY model_choice_key, id";

pub(crate) const FOLDERS: &str = "SELECT id, path, position, active FROM watched_folder \
     WHERE account_id = ?1 AND removed_at IS NULL ORDER BY position, id";

#[cfg(test)]
pub(crate) fn planned() -> Vec<(&'static str, String)> {
    vec![
        ("claim source probe", source_due_probe()),
        ("claim source upload", source_due_upload()),
        ("claim computation submit", computation_due_submit()),
        ("claim computation poll", computation_due_poll()),
        ("claim report request", part_due_request()),
        ("claim report poll", part_due_poll()),
        ("claim hold probe", HOLD_DUE_PROBE.to_owned()),
        ("claim open batches", BATCH_OPEN.to_owned()),
        ("list history body", history_body()),
        ("list history head", history_head()),
        ("list attention series", attention_series()),
        ("count attention series", ATTENTION_COUNT.to_owned()),
        ("get batch", batch_by_id()),
        ("list batch items", batch_items()),
        ("list item computations", ITEM_COMPUTATIONS.to_owned()),
        ("list folders", FOLDERS.to_owned()),
    ]
}

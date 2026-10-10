use mivia_domain::hold::Gate;

use super::gate_open;

pub(crate) const WAITING: &str = "SELECT r.id, b.report_settings, \
     EXISTS (SELECT 1 FROM source_file s WHERE s.batch_id = r.batch_id \
     AND s.state IN ('detected', 'ready', 'uploading')) \
     OR EXISTS (SELECT 1 FROM computation c WHERE c.batch_id = r.batch_id \
     AND c.model_choice_key = r.model_choice_key \
     AND c.state IN ('queued', 'submitting', 'computing')) \
     FROM report r INDEXED BY report_waiting JOIN batch b ON b.id = r.batch_id \
     WHERE r.account_id = ?1 AND r.state = 'waiting' ORDER BY r.id";

pub(crate) const INSERT_WAITING: &str = "INSERT INTO report (account_id, kind, batch_id, \
     model_choice_key, formats, language, state, created_at, app_version) \
     SELECT m.account_id, 'batch', m.batch_id, m.model_choice_key, '', '', 'waiting', ?2, \
     b.app_version FROM batch_model_choice m JOIN batch b ON b.id = m.batch_id \
     WHERE m.batch_id = ?1";

pub(crate) const BUILD_MEMBERS: &str = "SELECT c.id, c.state, c.reason, s.label, s.arrival_seq, \
     s.state = 'rejected' FROM computation c JOIN source_file s ON s.id = c.source_file_id \
     WHERE c.batch_id = ?1 AND c.model_choice_key = ?2 AND c.account_id = ?3";

pub(crate) const IN_FLIGHT: &str = "SELECT 1 FROM report_part INDEXED BY report_part_due \
     WHERE state = 'requested' AND server_report_id IS NOT NULL AND staging_path IS NULL LIMIT 1";

pub(crate) fn next_request() -> String {
    format!(
        "SELECT p.id, p.format, p.part_no, p.part_count, r.language, r.include_images, \
         r.decimal_separator, r.timezone, p.member_from, p.member_to, p.report_id, p.attempts \
         FROM report_part p INDEXED BY report_part_queue JOIN report r ON r.id = p.report_id \
         WHERE p.state = 'requested' AND p.server_report_id IS NULL AND p.next_attempt_at <= ?2 \
         AND r.account_id = ?1 AND {} \
         ORDER BY p.part_no, p.report_id, p.format DESC LIMIT 1",
        gate_open(Gate::ReportRequest)
    )
}

pub(crate) const PART_MEMBERS: &str = "SELECT m.seq, COALESCE(c.server_job_id, o.server_job_id), \
     m.label FROM report_member m JOIN computation c ON c.id = m.computation_id \
     LEFT JOIN computation o ON o.id = c.reused_from \
     WHERE m.report_id = ?1 AND m.seq > ?2 AND m.seq <= ?3 AND m.omitted IS NULL ORDER BY m.seq";

pub(crate) const OMITTED: &str = "SELECT label, omitted FROM report_member \
     WHERE report_id = ?1 AND omitted IS NOT NULL ORDER BY seq";

const POLLABLE: &str = "p.state = 'requested' AND p.server_report_id IS NOT NULL \
     AND p.staging_path IS NULL";

pub(crate) fn poll_due() -> String {
    format!(
        "SELECT p.id, p.server_report_id, p.format, COALESCE(p.requested_at, 0), p.rerequests, \
         p.split FROM report_part p JOIN report r ON r.id = p.report_id \
         WHERE {POLLABLE} AND p.next_poll_at <= ?2 AND r.account_id = ?1 AND {} \
         ORDER BY p.next_poll_at, p.id LIMIT 10",
        gate_open(Gate::ReportDownload)
    )
}

pub(crate) fn next_poll() -> String {
    format!(
        "SELECT min(p.next_poll_at) FROM report_part p JOIN report r ON r.id = p.report_id \
         WHERE {POLLABLE} AND r.account_id = ?1"
    )
}

pub(crate) const SAVE_DUE: &str = "SELECT p.id FROM report_part p INDEXED BY report_part_save \
     JOIN report r ON r.id = p.report_id \
     WHERE p.staging_path IS NOT NULL AND p.state IN ('requested', 'not_saved') \
     AND p.next_attempt_at <= ?2 AND r.account_id = ?1 ORDER BY p.next_attempt_at, p.id LIMIT 10";

pub(crate) const NEXT_SAVE: &str = "SELECT min(p.next_attempt_at) \
     FROM report_part p INDEXED BY report_part_save JOIN report r ON r.id = p.report_id \
     WHERE p.staging_path IS NOT NULL AND p.state IN ('requested', 'not_saved') \
     AND r.account_id = ?1";

pub(crate) const SAVE_ROW: &str = "SELECT p.format, p.part_no, p.part_count, r.language, \
     b.label, COALESCE(b.closed_at, r.created_at), \
     (SELECT names FROM model_catalogue k WHERE k.account_id = r.account_id \
     AND k.model_id = m.model_id AND k.customization_id = ''), \
     (SELECT names FROM model_catalogue k WHERE k.account_id = r.account_id \
     AND k.model_id = m.model_id AND k.customization_id = m.customization_id), \
     m.model_id, \
     f.variant_in_file_name OR (SELECT count(*) FROM batch_model_choice x \
     WHERE x.batch_id = b.id AND x.model_id = m.model_id) > 1, \
     f.path, f.report_folder_mode, f.report_folder_name, f.report_folder_path, f.probe_result, \
     (SELECT s.path FROM source_file s WHERE s.batch_id = b.id ORDER BY s.arrival_seq LIMIT 1), \
     p.staging_path, p.size, p.sha256, p.intended_name, m.customization_id, json_extract(b.report_settings, '$.oneOffFolder') \
     FROM report_part p JOIN report r ON r.id = p.report_id JOIN batch b ON b.id = r.batch_id \
     JOIN batch_model_choice m ON m.batch_id = b.id AND m.model_choice_key = r.model_choice_key \
     LEFT JOIN watched_folder f ON f.id = b.folder_id WHERE p.id = ?1 AND r.account_id = ?2";

pub(crate) const PART_HEAD: &str = "SELECT p.report_id, p.format, p.part_no, p.member_from, \
     p.member_to, p.split, p.rerequests FROM report_part p JOIN report r ON r.id = p.report_id \
     WHERE p.id = ?1 AND r.account_id = ?2";

pub(crate) const STALE_CANDIDATES: &str = "SELECT DISTINCT m.report_id FROM report_member m \
     JOIN report r ON r.id = m.report_id WHERE m.computation_id = ?1 AND r.stale = 0 \
     AND EXISTS (SELECT 1 FROM report_part p WHERE p.report_id = r.id AND p.state = 'saved')";

pub(crate) const RECOVER_POLLS: &str = "UPDATE report_part SET next_poll_at = ?2 \
     WHERE state = 'requested' AND server_report_id IS NOT NULL AND staging_path IS NULL \
     AND next_poll_at > ?2 AND report_id IN (SELECT id FROM report WHERE account_id = ?1)";

pub(crate) const RECOVER_SAVES: &str = "UPDATE report_part SET next_attempt_at = ?2 \
     WHERE staging_path IS NOT NULL AND state IN ('requested', 'not_saved') \
     AND next_attempt_at > ?2 AND report_id IN (SELECT id FROM report WHERE account_id = ?1)";

#[cfg(test)]
pub(crate) fn planned() -> Vec<(&'static str, String)> {
    vec![
        ("list waiting reports", WAITING.to_owned()),
        ("list build members", BUILD_MEMBERS.to_owned()),
        ("find report in flight", IN_FLIGHT.to_owned()),
        ("claim report part request", next_request()),
        ("list part members", PART_MEMBERS.to_owned()),
        ("list omitted members", OMITTED.to_owned()),
        ("claim report part poll", poll_due()),
        ("next report part poll", next_poll()),
        ("claim report part save", SAVE_DUE.to_owned()),
        ("next report part save", NEXT_SAVE.to_owned()),
        ("get save row", SAVE_ROW.to_owned()),
        ("find stale candidates", STALE_CANDIDATES.to_owned()),
    ]
}

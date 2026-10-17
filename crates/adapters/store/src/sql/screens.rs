use super::BATCH_COLUMNS;

pub(crate) const FOLDER_VIEWS: &str = "SELECT id, path, position, active, missing_since, \
     quiet_period, report_folder_mode, report_folder_name, report_folder_path, \
     waiting_network_since, marker_conflict FROM watched_folder WHERE account_id = ?1 AND removed_at IS NULL ORDER BY position, id";

pub(crate) fn last_closed_of_folder() -> String {
    format!(
        "SELECT {BATCH_COLUMNS} FROM batch INDEXED BY batch_hist_folder \
         WHERE account_id = ?1 AND folder_id = ?2 AND open = 0 AND deleted_at IS NULL \
         ORDER BY sort_at DESC, id DESC LIMIT 1"
    )
}

pub(crate) const HOLDS: &str =
    "SELECT reason, since FROM account_hold WHERE account_id = ?1 ORDER BY reason";

pub(crate) const PROBLEM_PART: &str = "SELECT p.id, max(p.state) FROM report r \
     INDEXED BY report_by_batch JOIN report_part p ON p.report_id = r.id \
     WHERE r.batch_id = ?1 AND r.replaced_by IS NULL AND r.deleted_at IS NULL AND p.state IN ('not_saved', 'failed')";

pub(crate) const FIRST_PROBLEM_IMAGE: &str = "SELECT f.id, f.label,      CASE WHEN f.state IN ('rejected', 'unreadable') THEN f.state ELSE      (SELECT c.state FROM computation c INDEXED BY computation_by_source      WHERE c.source_file_id = f.id AND c.state IN ('failed', 'unavailable', 'lost') LIMIT 1) END      FROM source_file f INDEXED BY source_file_by_batch WHERE f.batch_id = ?1      AND (f.state IN ('rejected', 'unreadable') OR f.id IN (SELECT c.source_file_id      FROM computation c INDEXED BY computation_by_batch WHERE c.batch_id = ?1      AND c.state IN ('failed', 'unavailable', 'lost'))) ORDER BY f.arrival_seq LIMIT 1";

pub(crate) const LATEST_SAVED_PART: &str = "SELECT p.id, max(p.format) FROM report r \
     INDEXED BY report_by_batch JOIN report_part p ON p.report_id = r.id \
     WHERE r.batch_id = ?1 AND r.replaced_by IS NULL AND r.deleted_at IS NULL AND p.state = 'saved'";

pub(crate) const SERIES_REPORTS: &str = "SELECT r.id, r.model_choice_key, r.state, r.stale, \
     r.replaced_by, EXISTS (SELECT 1 FROM report_part p WHERE p.report_id = r.id \
     AND p.moved_to_replaced_at IS NOT NULL) FROM report r INDEXED BY report_by_batch \
     WHERE r.batch_id = ?2 AND r.account_id = ?1 AND r.deleted_at IS NULL ORDER BY r.id";

pub(crate) const REPORT_PARTS: &str = "SELECT id, part_no, part_count, format, member_from, \
     member_to, state, coalesce(save_problem, error), file_path FROM report_part \
     WHERE report_id = ?1 ORDER BY part_no, format";

pub(crate) const BATCH_ANALYSES: &str = "SELECT m.model_choice_key, m.model_id, \
     coalesce((SELECT k.names FROM model_catalogue k WHERE k.account_id = m.account_id \
     AND k.model_id = m.model_id AND k.customization_id = ifnull(m.customization_id, '')), \
     m.names) FROM batch_model_choice m WHERE m.batch_id = ?1 ORDER BY m.model_choice_key";

pub(crate) const ACCOUNT: &str =
    "SELECT display_name, server_url, server_account_id, email FROM account WHERE id = ?1";

pub(crate) const SETTINGS: &str =
    "SELECT json FROM settings WHERE scope = ?1 AND ifnull(account_id, 0) = 0";

#[cfg(test)]
pub(crate) fn planned() -> Vec<(&'static str, String)> {
    vec![
        ("list folder views", FOLDER_VIEWS.to_owned()),
        ("last closed series of folder", last_closed_of_folder()),
        ("list holds", HOLDS.to_owned()),
        ("find problem part of series", PROBLEM_PART.to_owned()),
        (
            "first problem image of series",
            FIRST_PROBLEM_IMAGE.to_owned(),
        ),
        ("latest saved part of series", LATEST_SAVED_PART.to_owned()),
        ("list series reports", SERIES_REPORTS.to_owned()),
        ("list report parts", REPORT_PARTS.to_owned()),
        ("list batch analyses", BATCH_ANALYSES.to_owned()),
        ("get account", ACCOUNT.to_owned()),
        ("get settings", SETTINGS.to_owned()),
    ]
}

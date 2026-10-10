use super::BATCH_COLUMNS;

pub(crate) const FOLDER_VIEWS: &str = "SELECT id, path, position, active, missing_since, \
     quiet_period, report_folder_mode, report_folder_name, report_folder_path \
     FROM watched_folder WHERE account_id = ?1 AND removed_at IS NULL ORDER BY position, id";

pub(crate) fn last_closed_of_folder() -> String {
    format!(
        "SELECT {BATCH_COLUMNS} FROM batch INDEXED BY batch_hist_folder \
         WHERE account_id = ?1 AND folder_id = ?2 AND open = 0 \
         ORDER BY sort_at DESC, id DESC LIMIT 1"
    )
}

pub(crate) const HOLDS: &str =
    "SELECT reason, since FROM account_hold WHERE account_id = ?1 ORDER BY reason";

pub(crate) const PROBLEM_PART: &str = "SELECT p.id, max(p.state) FROM report r \
     INDEXED BY report_by_batch JOIN report_part p ON p.report_id = r.id \
     WHERE r.batch_id = ?1 AND r.replaced_by IS NULL AND p.state IN ('not_saved', 'failed')";

pub(crate) const LATEST_SAVED_PART: &str = "SELECT p.id, max(p.format) FROM report r \
     INDEXED BY report_by_batch JOIN report_part p ON p.report_id = r.id \
     WHERE r.batch_id = ?1 AND r.replaced_by IS NULL AND p.state = 'saved'";

pub(crate) const SERIES_REPORTS: &str = "SELECT id, model_choice_key, state, stale FROM report \
     INDEXED BY report_by_batch WHERE batch_id = ?2 AND account_id = ?1 \
     AND replaced_by IS NULL ORDER BY id";

pub(crate) const REPORT_PARTS: &str = "SELECT id, part_no, part_count, format, member_from, \
     member_to, state, coalesce(save_problem, error), file_path FROM report_part \
     WHERE report_id = ?1 ORDER BY part_no, format";

pub(crate) const BATCH_ANALYSES: &str = "SELECT m.model_choice_key, m.model_id, \
     coalesce((SELECT k.names FROM model_catalogue k WHERE k.account_id = m.account_id \
     AND k.model_id = m.model_id AND k.customization_id = ifnull(m.customization_id, '')), \
     m.names) FROM batch_model_choice m WHERE m.batch_id = ?1 ORDER BY m.model_choice_key";

pub(crate) const ACCOUNT: &str =
    "SELECT display_name, server_url, server_account_id FROM account WHERE id = ?1";

pub(crate) const GLOBAL_SETTINGS: &str =
    "SELECT json FROM settings WHERE scope = 'global' AND ifnull(account_id, 0) = 0";

#[cfg(test)]
pub(crate) fn planned() -> Vec<(&'static str, String)> {
    vec![
        ("list folder views", FOLDER_VIEWS.to_owned()),
        ("last closed series of folder", last_closed_of_folder()),
        ("list holds", HOLDS.to_owned()),
        ("find problem part of series", PROBLEM_PART.to_owned()),
        ("latest saved part of series", LATEST_SAVED_PART.to_owned()),
        ("list series reports", SERIES_REPORTS.to_owned()),
        ("list report parts", REPORT_PARTS.to_owned()),
        ("list batch analyses", BATCH_ANALYSES.to_owned()),
        ("get account", ACCOUNT.to_owned()),
        ("get global settings", GLOBAL_SETTINGS.to_owned()),
    ]
}

use mivia_domain::hold::Gate;

use super::gate_open;

const UNGATED: &str = "?4 > 0";

fn gate(gated: bool, gate: Gate) -> String {
    if gated {
        gate_open(gate)
    } else {
        UNGATED.to_owned()
    }
}

pub(crate) fn upload_due(gated: bool) -> String {
    format!(
        "SELECT s.id, s.batch_id, b.origin = 'one_off', b.opened_at, s.arrival_seq, s.path, \
         f.path, s.size, s.fingerprint, s.attempts \
         FROM source_file s JOIN batch b ON b.id = s.batch_id \
         LEFT JOIN watched_folder f ON f.id = b.folder_id \
         WHERE s.account_id = ?1 AND s.state = 'ready' AND s.next_attempt_at <= ?2 \
         AND s.id NOT IN (SELECT value FROM json_each(?3)) \
         AND s.batch_id NOT IN (SELECT value FROM json_each(?5)) \
         AND EXISTS (SELECT 1 FROM computation c WHERE c.source_file_id = s.id \
         AND c.state = 'queued') AND {} \
         ORDER BY s.next_attempt_at, s.id LIMIT ?4",
        gate(gated, Gate::Upload)
    )
}

pub(crate) fn submit_due(gated: bool) -> String {
    format!(
        "SELECT c.id, c.batch_id, b.origin = 'one_off', b.opened_at, s.arrival_seq, \
         c.model_choice_key, s.server_image_id, c.attempts \
         FROM computation c JOIN source_file s ON s.id = c.source_file_id \
         JOIN batch b ON b.id = c.batch_id \
         WHERE c.state IN ('queued', 'submitting', 'computing') AND c.state = 'queued' \
         AND c.account_id = ?1 AND c.next_attempt_at <= ?2 \
         AND s.state = 'uploaded' AND s.reason IS NULL \
         AND c.id NOT IN (SELECT value FROM json_each(?3)) \
         AND c.batch_id NOT IN (SELECT value FROM json_each(?5)) AND {} \
         ORDER BY c.next_attempt_at, c.id LIMIT ?4",
        gate(gated, Gate::Submit)
    )
}

pub(crate) const NEXT_POLL: &str = "SELECT min(next_poll_at) FROM computation \
     WHERE state = 'computing' AND account_id = ?1";

pub(crate) fn poll_due() -> String {
    format!(
        "SELECT id, server_job_id, COALESCE(submitted_at, 0), resubmitted FROM computation \
         WHERE state = 'computing' AND account_id = ?1 AND next_poll_at <= ?2 \
         AND id NOT IN (SELECT value FROM json_each(?3)) AND {} \
         ORDER BY next_poll_at, id LIMIT ?4",
        gate_open(Gate::StatusPoll)
    )
}

pub(crate) const RECOVERY_KEYS: &str = "SELECT submit_attempt_key \
     FROM computation INDEXED BY computation_claim \
     WHERE state IN ('queued', 'submitting', 'computing') AND state = 'submitting' \
     AND account_id = ?1 AND recovery_due <= ?2 AND next_attempt_at <= ?2 \
     AND submit_attempt_key NOT IN (SELECT value FROM json_each(?3)) \
     ORDER BY next_attempt_at, id LIMIT ?4";

pub(crate) const ATTEMPT: &str = "SELECT model_choice_key, started_at, server_offset_ms \
     FROM submit_attempt WHERE key = ?1 AND account_id = ?2";

pub(crate) const ATTEMPT_MEMBERS: &str = "SELECT c.id, s.server_image_id, c.attempts FROM computation c \
     JOIN source_file s ON s.id = c.source_file_id \
     WHERE c.submit_attempt_key = ?1 AND c.state = 'submitting' ORDER BY c.id";

pub(crate) const INSERT_ATTEMPT: &str = "INSERT INTO submit_attempt (key, account_id, \
     model_choice_key, image_ids, started_at, server_offset_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
     ON CONFLICT (key) DO NOTHING";

pub(crate) const JOB_KNOWN: &str = "SELECT 1 FROM computation WHERE server_job_id = ?1 LIMIT 1";

pub(crate) const FILES_WITH_IMAGE: &str =
    "SELECT id FROM source_file WHERE account_id = ?1 AND server_image_id = ?2";

pub(crate) const COMPUTING_COUNT: &str =
    "SELECT count(*) FROM computation WHERE state = 'computing' AND account_id = ?1";

pub(crate) const UPLOADING_FILES: &str =
    "SELECT id FROM source_file WHERE account_id = ?1 AND state = 'uploading'";

pub(crate) const QUEUED_CHOICES: &str = "SELECT id, model_choice_key FROM computation \
     WHERE state IN ('queued', 'submitting', 'computing') AND state = 'queued' \
     AND account_id = ?1";

pub(crate) const PREVIEW_SOURCE: &str =
    "SELECT path FROM source_file WHERE id = ?1 AND account_id = ?2";

pub(crate) const PREVIEW_JOB: &str = "SELECT server_job_id FROM computation      WHERE id = ?1 AND account_id = ?2 AND server_job_id IS NOT NULL";

pub(crate) const PREVIEW_MODEL: &str = "SELECT m.model_id FROM computation c      JOIN batch_model_choice m ON m.batch_id = c.batch_id      AND m.model_choice_key = c.model_choice_key      WHERE c.id = ?1 AND c.account_id = ?2";

#[cfg(test)]
pub(crate) fn planned() -> Vec<(&'static str, String)> {
    vec![
        ("claim pipeline upload", upload_due(true)),
        ("claim upload canary", upload_due(false)),
        ("claim pipeline submit", submit_due(true)),
        ("claim submit canary", submit_due(false)),
        ("next poll", NEXT_POLL.to_owned()),
        ("claim pipeline poll", poll_due()),
        ("claim recovery attempts", RECOVERY_KEYS.to_owned()),
        ("get submit attempt", ATTEMPT.to_owned()),
        ("list attempt members", ATTEMPT_MEMBERS.to_owned()),
        ("find job reference", JOB_KNOWN.to_owned()),
        ("find files of image", FILES_WITH_IMAGE.to_owned()),
        ("count computing", COMPUTING_COUNT.to_owned()),
        ("list uploading files", UPLOADING_FILES.to_owned()),
        ("list queued choices", QUEUED_CHOICES.to_owned()),
        ("get preview source path", PREVIEW_SOURCE.to_owned()),
        ("get preview job", PREVIEW_JOB.to_owned()),
        ("get preview model", PREVIEW_MODEL.to_owned()),
    ]
}

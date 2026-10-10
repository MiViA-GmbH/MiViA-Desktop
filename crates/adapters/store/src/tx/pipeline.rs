use mivia_domain::computation::ComputationEvent;
use mivia_domain::ids::{BatchId, ComputationId, SourceFileId, Timestamp};
use mivia_domain::source_file::SourceFileEvent;
use mivia_ports::store::pipeline::{
    AttemptRow, CatalogueRow, Clamp, JobRecord, NewAttempt, PipelineRepo, PollRow, Retry,
    SubmitRow, UploadRow,
};
use mivia_ports::store::{ComputationRepo, SourceFileRepo, TxError};
use rusqlite::params;

use super::claims::{poll_row, raw_upload, serde_like_list, submit_row, upload_row};
use super::{SqlTx, id_list};
use crate::sql::{CLAIM_LIMIT, pipeline as sql};

impl PipelineRepo for SqlTx<'_> {
    fn upload_due(
        &mut self,
        now: Timestamp,
        exclude: &[SourceFileId],
        skip_batches: &[BatchId],
    ) -> Result<Vec<UploadRow>, TxError> {
        let values = params![
            self.account(),
            now.0,
            id_list(exclude, |id| id.0),
            CLAIM_LIMIT,
            id_list(skip_batches, |id| id.0)
        ];
        let raws = self.rows(&sql::upload_due(true), values, raw_upload)?;
        raws.into_iter().map(upload_row).collect()
    }

    fn upload_canary(&mut self, now: Timestamp) -> Result<Vec<UploadRow>, TxError> {
        let values = params![self.account(), now.0, "[]", CLAIM_LIMIT, "[]"];
        let raws = self.rows(&sql::upload_due(false), values, raw_upload)?;
        raws.into_iter().map(upload_row).collect()
    }

    fn uploaded(
        &mut self,
        id: SourceFileId,
        image_id: &str,
        now: Timestamp,
    ) -> Result<(), TxError> {
        SourceFileRepo::apply(self, id, SourceFileEvent::UploadSucceeded, now)?;
        self.run(
            "UPDATE source_file SET server_image_id = ?2, attempts = 0, first_busy_at = NULL \
             WHERE id = ?1",
            params![id.0, image_id],
        )
    }

    fn upload_retry(
        &mut self,
        id: SourceFileId,
        retry: Retry,
        now: Timestamp,
    ) -> Result<(), TxError> {
        SourceFileRepo::apply(self, id, SourceFileEvent::UploadInterrupted, now)?;
        self.run(
            "UPDATE source_file SET next_attempt_at = ?2, attempts = ?3 WHERE id = ?1",
            params![id.0, retry.at.0, retry.attempts],
        )
    }

    fn file_failed(
        &mut self,
        id: SourceFileId,
        event: SourceFileEvent,
        now: Timestamp,
    ) -> Result<(), TxError> {
        let state = SourceFileRepo::apply(self, id, event, now)?;
        if state.is_terminal() {
            self.cancel_work(id, now)?;
        }
        Ok(())
    }

    fn image_gone(&mut self, image_id: &str, now: Timestamp) -> Result<(), TxError> {
        let ids: Vec<i64> =
            self.column(sql::FILES_WITH_IMAGE, params![self.account(), image_id])?;
        for id in ids {
            SourceFileRepo::apply(self, SourceFileId(id), SourceFileEvent::ImageGone, now)?;
        }
        Ok(())
    }

    fn submit_due(
        &mut self,
        now: Timestamp,
        exclude: &[ComputationId],
        skip_batches: &[BatchId],
    ) -> Result<Vec<SubmitRow>, TxError> {
        let values = params![
            self.account(),
            now.0,
            id_list(exclude, |id| id.0),
            CLAIM_LIMIT,
            id_list(skip_batches, |id| id.0)
        ];
        self.rows(&sql::submit_due(true), values, submit_row)
    }

    fn submit_canary(&mut self, now: Timestamp) -> Result<Vec<SubmitRow>, TxError> {
        let values = params![self.account(), now.0, "[]", 1, "[]"];
        self.rows(&sql::submit_due(false), values, submit_row)
    }

    fn next_poll(&mut self) -> Result<Option<Timestamp>, TxError> {
        let next: Vec<Option<i64>> = self.column(sql::NEXT_POLL, [self.account()])?;
        Ok(next.into_iter().flatten().next().map(Timestamp))
    }

    fn begin_attempt(&mut self, attempt: &NewAttempt, now: Timestamp) -> Result<(), TxError> {
        let images: Vec<&str> = attempt
            .members
            .iter()
            .map(|(_, image)| image.as_str())
            .collect();
        let image_ids = format!("[\"{}\"]", images.join("\",\""));
        self.run(
            sql::INSERT_ATTEMPT,
            params![
                attempt.key,
                self.account(),
                attempt.choice_key,
                image_ids,
                attempt.started_at.0,
                attempt.server_offset_ms
            ],
        )?;
        for (id, _) in &attempt.members {
            let queued = ComputationRepo::apply(self, *id, ComputationEvent::SubmitClaimed, now);
            match queued {
                Ok(_) | Err(TxError::Illegal(_)) => {}
                Err(other) => return Err(other),
            }
            self.run(
                "UPDATE computation SET submit_attempt_key = ?2, recovery_due = NULL \
                 WHERE id = ?1",
                params![id.0, attempt.key],
            )?;
        }
        Ok(())
    }

    fn record_job(
        &mut self,
        id: ComputationId,
        job: &JobRecord,
        now: Timestamp,
    ) -> Result<(), TxError> {
        ComputationRepo::apply(self, id, ComputationEvent::Job(job.outcome), now)?;
        self.run(
            "UPDATE computation SET server_job_id = ?2, result_id = ?3, \
             submitted_at = COALESCE(submitted_at, ?4), next_poll_at = ?5, recovery_due = NULL \
             WHERE id = ?1",
            params![
                id.0,
                job.job_id,
                job.result_id,
                job.submitted_at.0,
                job.next_poll_at.0
            ],
        )
    }

    fn computation_event(
        &mut self,
        id: ComputationId,
        event: ComputationEvent,
        now: Timestamp,
    ) -> Result<(), TxError> {
        ComputationRepo::apply(self, id, event, now)?;
        if event == ComputationEvent::Ambiguous {
            self.run(
                "UPDATE computation SET recovery_due = ?2 WHERE id = ?1",
                params![id.0, now.0],
            )?;
        }
        if matches!(event, ComputationEvent::JobGone { resubmitted: false }) {
            self.run(
                "UPDATE computation SET resubmitted = 1, server_job_id = NULL WHERE id = ?1",
                [id.0],
            )?;
        }
        Ok(())
    }

    fn computation_retry(&mut self, id: ComputationId, retry: Retry) -> Result<(), TxError> {
        self.run(
            "UPDATE computation SET next_attempt_at = ?2, attempts = ?3 WHERE id = ?1",
            params![id.0, retry.at.0, retry.attempts],
        )
    }

    fn recovery_due(
        &mut self,
        now: Timestamp,
        exclude: &[String],
    ) -> Result<Vec<AttemptRow>, TxError> {
        let excluded = serde_like_list(exclude);
        let mut keys: Vec<String> = self.column(
            sql::RECOVERY_KEYS,
            params![self.account(), now.0, excluded, CLAIM_LIMIT],
        )?;
        keys.sort();
        keys.dedup();
        keys.into_iter().map(|key| self.attempt(key)).collect()
    }

    fn job_known(&mut self, job_id: &str) -> Result<bool, TxError> {
        let found: Vec<i64> = self.column(sql::JOB_KNOWN, [job_id])?;
        Ok(!found.is_empty())
    }

    fn poll_due(
        &mut self,
        now: Timestamp,
        exclude: &[ComputationId],
        limit: u32,
    ) -> Result<Vec<PollRow>, TxError> {
        let values = params![self.account(), now.0, id_list(exclude, |id| id.0), limit];
        self.rows(&sql::poll_due(), values, poll_row)
    }

    fn set_next_poll(&mut self, id: ComputationId, at: Timestamp) -> Result<(), TxError> {
        self.run(
            "UPDATE computation SET next_poll_at = ?3 WHERE id = ?1 AND account_id = ?2",
            params![id.0, self.account(), at.0],
        )
    }

    fn computing_count(&mut self) -> Result<u64, TxError> {
        let counts: Vec<i64> = self.column(sql::COMPUTING_COUNT, [self.account()])?;
        Ok(counts
            .first()
            .map_or(0, |n| u64::try_from(*n).unwrap_or_default()))
    }

    fn recover_after_restart(&mut self, now: Timestamp) -> Result<(), TxError> {
        self.restart_rows(now)
    }

    fn clamp_schedule(&mut self, clamp: Clamp) -> Result<(), TxError> {
        self.clamp_rows(clamp)
    }

    fn replace_catalogue(&mut self, rows: &[CatalogueRow], now: Timestamp) -> Result<(), TxError> {
        self.catalogue_rows(rows, now)
    }

    fn queued_choices(&mut self) -> Result<Vec<(ComputationId, String)>, TxError> {
        self.rows(sql::QUEUED_CHOICES, [self.account()], |row| {
            Ok((ComputationId(row.get(0)?), row.get(1)?))
        })
    }
}

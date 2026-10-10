use std::collections::BTreeMap;

use mivia_domain::computation::{ComputationEvent, JobOutcome, PendingVerdict, pending_verdict};
use mivia_domain::hold::{Access, Gate};
use mivia_domain::ids::{ComputationId, MINUTE_MS, Timestamp};
use mivia_domain::polling::{poll_delay_ms, status_request_gap_ms};
use mivia_ports::server::{MAX_STATUS_IDS, ServerJob, ServerJobStatus};
use mivia_ports::store::pipeline::{JobRecord, PollRow};
use mivia_ports::telemetry::{EventKind, ProductEvent};

use super::effects::{Call, Item};
use super::outcome::record;
use super::{Pipeline, PipelineError, Tick};

const OMISSION_GAP_MS: i64 = MINUTE_MS;
const MISMATCH_BACKOFF_MS: i64 = 5 * MINUTE_MS;

enum Change {
    Record(JobRecord),
    Event(ComputationEvent),
    PollAt(Timestamp),
}

impl Pipeline {
    pub async fn poll_tick(&self) -> Result<Tick, PipelineError> {
        let floor = match self.access(Gate::StatusPoll).await? {
            Access::Blocked => return Ok(Tick::Idle),
            Access::Throttled { min_interval_ms } => min_interval_ms,
            Access::Allowed => 0,
        };
        let (computing, next) = self
            .write(|tx, scope| {
                let repo = tx.pipeline(scope);
                Ok((repo.computing_count()?, repo.next_poll()?))
            })
            .await?;
        let now = self.now();
        let gap = floor.max(status_request_gap_ms(computing));
        let last = self.schedule.lock().last_status_request;
        if let Some(allowed) = last.map(|at| at.plus_ms(gap)).filter(|at| *at > now) {
            return Ok(Tick::DueAt(allowed));
        }
        let limit = u32::try_from(MAX_STATUS_IDS).unwrap_or(u32::MAX);
        let rows = self
            .write(move |tx, scope| tx.pipeline(scope).poll_due(now, &[], limit))
            .await?;
        if rows.is_empty() {
            return Ok(next.map_or(Tick::Idle, Tick::DueAt));
        }
        self.schedule.lock().last_status_request = Some(now);
        let changes = self.request_statuses(&rows, gap).await?;
        self.apply_changes(changes).await?;
        Ok(Tick::Busy)
    }

    async fn request_statuses(
        &self,
        rows: &[PollRow],
        gap: i64,
    ) -> Result<Vec<(ComputationId, Change)>, PipelineError> {
        let now = self.now();
        let ids = rows.iter().map(|row| row.job_id.clone()).collect();
        let error = match self.ports.server.job_statuses(ids).await {
            Ok(jobs) => return Ok(self.status_changes(rows, &jobs).await),
            Err(error) => error,
        };
        let retry = self
            .infrastructure(&error, Call::Other, Item::new("status", 0))
            .await?;
        let at = retry.map_or_else(|| now.plus_ms(gap.max(MINUTE_MS)), |retry| retry.at);
        let at = at.max(now.plus_ms(gap));
        Ok(rows
            .iter()
            .map(|row| (row.id, Change::PollAt(at)))
            .collect())
    }

    async fn status_changes(
        &self,
        rows: &[PollRow],
        jobs: &[ServerJob],
    ) -> Vec<(ComputationId, Change)> {
        let now = self.now();
        let found: BTreeMap<&str, &ServerJob> =
            jobs.iter().map(|job| (job.job_id.as_str(), job)).collect();
        let missing = rows
            .iter()
            .filter(|row| !found.contains_key(row.job_id.as_str()))
            .count();
        if rows.len() > 1 && missing * 2 > rows.len() {
            tracing::warn!(missing, asked = rows.len(), "status.suspectedKeyMismatch");
            let later = now.plus_ms(MISMATCH_BACKOFF_MS);
            return rows
                .iter()
                .map(|row| (row.id, Change::PollAt(later)))
                .collect();
        }
        let mut changes = Vec::new();
        for row in rows {
            let change = match found.get(row.job_id.as_str()) {
                Some(job) => {
                    self.schedule.lock().omitted.remove(&row.id);
                    status_change(row, job, now)
                }
                None => self.omission(row).await,
            };
            changes.push((row.id, change));
        }
        changes
    }

    async fn omission(&self, row: &PollRow) -> Change {
        let now = self.now();
        let first = *self.schedule.lock().omitted.entry(row.id).or_insert(now);
        if now.ms_since(first) < OMISSION_GAP_MS {
            return Change::PollAt(first.plus_ms(OMISSION_GAP_MS));
        }
        self.schedule.lock().omitted.remove(&row.id);
        match self.ports.server.job_exists(&row.job_id).await {
            Ok(false) => Change::Event(ComputationEvent::JobGone {
                resubmitted: row.resubmitted,
            }),
            Ok(true) | Err(_) => {
                Change::PollAt(now.plus_ms(poll_delay_ms(now.ms_since(row.submitted_at))))
            }
        }
    }

    async fn apply_changes(
        &self,
        changes: Vec<(ComputationId, Change)>,
    ) -> Result<(), PipelineError> {
        let now = self.now();
        let settled: Vec<ProductEvent> = changes
            .iter()
            .filter_map(|(id, change)| settled(*id, change, now))
            .collect();
        self.write(move |tx, scope| {
            let repo = tx.pipeline(scope);
            for (id, change) in changes {
                match change {
                    Change::Record(job) => repo.record_job(id, &job, now)?,
                    Change::Event(event) => repo.computation_event(id, event, now)?,
                    Change::PollAt(at) => repo.set_next_poll(id, at)?,
                }
            }
            Ok(())
        })
        .await?;
        for event in settled {
            self.ports.telemetry.record(event);
        }
        self.wakers.submitter.notify_one();
        self.wakers.reports.notify_one();
        Ok(())
    }
}

fn settled(id: ComputationId, change: &Change, now: Timestamp) -> Option<ProductEvent> {
    let event = ProductEvent::new(EventKind::ComputationSettled).entity(
        "computation_id",
        "computation",
        id.0,
    );
    match change {
        Change::Record(job) => {
            let took = u64::try_from(now.ms_since(job.submitted_at)).unwrap_or(0);
            match job.outcome {
                JobOutcome::Done { reused } => Some(
                    event
                        .tag("state", "done")
                        .flag("reused", reused)
                        .count("duration_ms", took),
                ),
                JobOutcome::Failed(_) => {
                    Some(event.tag("state", "failed").count("duration_ms", took))
                }
                JobOutcome::Pending => None,
            }
        }
        Change::Event(ComputationEvent::JobGone { .. }) => Some(event.tag("state", "lost")),
        Change::Event(ComputationEvent::TimedOut(_)) => Some(event.tag("state", "failed")),
        Change::Event(_) | Change::PollAt(_) => None,
    }
}

fn status_change(row: &PollRow, job: &ServerJob, now: Timestamp) -> Change {
    let unknown = matches!(job.status, ServerJobStatus::Unknown(_));
    let pending = matches!(job.status, ServerJobStatus::Pending) || unknown;
    if !pending {
        return Change::Record(record(job, row.submitted_at, now));
    }
    match pending_verdict(row.submitted_at, now, unknown) {
        PendingVerdict::TimedOut(reason) => Change::Event(ComputationEvent::TimedOut(reason)),
        PendingVerdict::Waiting | PendingVerdict::Slow => {
            Change::PollAt(now.plus_ms(poll_delay_ms(now.ms_since(row.submitted_at))))
        }
    }
}

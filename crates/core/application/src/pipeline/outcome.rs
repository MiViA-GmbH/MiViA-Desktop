use mivia_domain::computation::{FailReason, JobOutcome};
use mivia_domain::ids::Timestamp;
use mivia_domain::polling::poll_delay_ms;
use mivia_ports::server::{ServerJob, ServerJobStatus};
use mivia_ports::store::pipeline::JobRecord;

pub(super) fn outcome(job: &ServerJob) -> JobOutcome {
    match (&job.status, job.has_result) {
        (ServerJobStatus::Pending | ServerJobStatus::Unknown(_), _) => JobOutcome::Pending,
        (ServerJobStatus::New, true) => JobOutcome::Done { reused: false },
        (ServerJobStatus::Cached, true) => JobOutcome::Done { reused: true },
        (ServerJobStatus::New | ServerJobStatus::Cached, false) => {
            JobOutcome::Failed(FailReason::NoResult)
        }
        (ServerJobStatus::Failed, _) => JobOutcome::Failed(FailReason::ServerFailed),
    }
}

pub(super) fn record(job: &ServerJob, submitted_at: Timestamp, now: Timestamp) -> JobRecord {
    JobRecord {
        job_id: job.job_id.clone(),
        outcome: outcome(job),
        result_id: job.result_id.clone(),
        submitted_at,
        next_poll_at: now.plus_ms(poll_delay_ms(now.ms_since(submitted_at))),
    }
}

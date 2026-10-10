use crate::ids::{HOUR_MS, Timestamp};
use crate::transition::IllegalTransition;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ComputationState {
    Queued,
    Submitting,
    Computing,
    Done { reused: bool },
    Failed(FailReason),
    Unavailable,
    Lost,
    Cancelled,
    Superseded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FailReason {
    ServerFailed,
    NoResult,
    Refused,
    Contract,
    ServerTimeout,
    UnknownServerStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum JobOutcome {
    Pending,
    Done { reused: bool },
    Failed(FailReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ComputationEvent {
    ChoiceUnavailable,
    SubmitClaimed,
    Job(JobOutcome),
    Ambiguous,
    Refused { choice_gone: bool },
    ContractError,
    JobGone { resubmitted: bool },
    TimedOut(FailReason),
    Cancel,
    RunAgain,
}

impl ComputationState {
    pub const fn is_terminal(self) -> bool {
        !matches!(self, Self::Queued | Self::Submitting | Self::Computing)
    }

    pub const fn apply(self, event: ComputationEvent) -> Result<Self, IllegalTransition> {
        use ComputationEvent as E;
        match (self, event) {
            (Self::Submitting | Self::Computing, E::Job(outcome)) => Ok(job(outcome)),
            (Self::Queued, E::SubmitClaimed) | (Self::Submitting, E::Ambiguous) => {
                Ok(Self::Submitting)
            }
            (Self::Queued, E::ChoiceUnavailable)
            | (Self::Submitting, E::Refused { choice_gone: true }) => Ok(Self::Unavailable),
            (Self::Submitting, E::Refused { choice_gone: false }) => {
                Ok(Self::Failed(FailReason::Refused))
            }
            (Self::Submitting, E::ContractError) => Ok(Self::Failed(FailReason::Contract)),
            (Self::Computing, E::JobGone { resubmitted: false }) => Ok(Self::Queued),
            (Self::Computing, E::JobGone { resubmitted: true }) => Ok(Self::Lost),
            (Self::Computing, E::TimedOut(reason)) => Ok(Self::Failed(reason)),
            (Self::Queued | Self::Submitting | Self::Computing, E::Cancel) => Ok(Self::Cancelled),
            (Self::Done { .. }, E::RunAgain) => Ok(Self::Superseded),
            _ => Err(IllegalTransition("computation")),
        }
    }
}

const fn job(outcome: JobOutcome) -> ComputationState {
    match outcome {
        JobOutcome::Pending => ComputationState::Computing,
        JobOutcome::Done { reused } => ComputationState::Done { reused },
        JobOutcome::Failed(reason) => ComputationState::Failed(reason),
    }
}

pub const SLOW_AFTER_MS: i64 = 6 * HOUR_MS;
pub const PENDING_LIMIT_MS: i64 = 72 * HOUR_MS;
pub const UNKNOWN_STATUS_LIMIT_MS: i64 = 24 * HOUR_MS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingVerdict {
    Waiting,
    Slow,
    TimedOut(FailReason),
}

pub fn pending_verdict(
    submitted_at: Timestamp,
    now: Timestamp,
    unknown_status: bool,
) -> PendingVerdict {
    let age = now.ms_since(submitted_at);
    if unknown_status && age >= UNKNOWN_STATUS_LIMIT_MS {
        return PendingVerdict::TimedOut(FailReason::UnknownServerStatus);
    }
    if age >= PENDING_LIMIT_MS {
        return PendingVerdict::TimedOut(FailReason::ServerTimeout);
    }
    if age >= SLOW_AFTER_MS {
        return PendingVerdict::Slow;
    }
    PendingVerdict::Waiting
}

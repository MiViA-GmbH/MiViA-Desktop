use std::ops::Range;

use crate::computation::ComputationState;
use crate::ids::{HOUR_MS, MINUTE_MS, SECOND_MS};
use crate::transition::IllegalTransition;

pub const MAX_PART_MEMBERS: usize = 100;
pub const MAX_REREQUESTS: u32 = 3;
pub const PENDING_GIVE_UP_MS: i64 = 2 * HOUR_MS;
pub const SAVE_RETRY_MS: i64 = 5 * MINUTE_MS;
pub const REPORTS_V2_MIN_SERVER_VERSION: Option<&str> = None;
const FAST_POLL_MS: i64 = 3 * SECOND_MS;
const SLOW_POLL_MS: i64 = 15 * SECOND_MS;

pub const fn poll_delay_ms(age_ms: i64) -> i64 {
    if age_ms < MINUTE_MS {
        FAST_POLL_MS
    } else {
        SLOW_POLL_MS
    }
}

pub fn supports_v2(server: &str, min: Option<&str>) -> bool {
    min.is_some_and(|min| version_numbers(server) >= version_numbers(min))
}

fn version_numbers(version: &str) -> Vec<u64> {
    let release = version.split(['-', '+']).next().unwrap_or_default();
    release
        .split('.')
        .map(|part| {
            let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().unwrap_or_default()
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OmittedReason {
    Failed,
    Rejected,
    Unavailable,
    Lost,
    Cancelled,
}

impl OmittedReason {
    pub const ALL: [Self; 5] = [
        Self::Failed,
        Self::Rejected,
        Self::Unavailable,
        Self::Lost,
        Self::Cancelled,
    ];

    pub const fn code(self) -> &'static str {
        match self {
            Self::Failed => "FAILED",
            Self::Rejected => "REJECTED",
            Self::Unavailable => "UNAVAILABLE",
            Self::Lost => "LOST",
            Self::Cancelled => "CANCELLED",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|reason| reason.code() == code)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Membership {
    Included,
    Omitted(OmittedReason),
    Excluded,
}

pub const fn membership(state: ComputationState, file_rejected: bool) -> Membership {
    use ComputationState as S;
    match state {
        S::Done { .. } => Membership::Included,
        S::Failed(_) => Membership::Omitted(OmittedReason::Failed),
        S::Unavailable => Membership::Omitted(OmittedReason::Unavailable),
        S::Lost => Membership::Omitted(OmittedReason::Lost),
        S::Cancelled if file_rejected => Membership::Omitted(OmittedReason::Rejected),
        S::Cancelled => Membership::Omitted(OmittedReason::Cancelled),
        S::Queued | S::Submitting | S::Computing | S::Superseded => Membership::Excluded,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SaveProblem {
    Unreachable,
    Denied,
    DiskFull,
    Busy,
    Moved,
}

impl SaveProblem {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Unreachable => "unreachable",
            Self::Denied => "denied",
            Self::DiskFull => "disk_full",
            Self::Busy => "busy",
            Self::Moved => "moved",
        }
    }
}

pub fn split_parts(members: usize) -> Vec<Range<usize>> {
    (0..members.div_ceil(MAX_PART_MEMBERS))
        .map(|part| {
            let start = part * MAX_PART_MEMBERS;
            start..members.min(start + MAX_PART_MEMBERS)
        })
        .collect()
}

pub fn halve(part: &Range<usize>) -> Option<(Range<usize>, Range<usize>)> {
    let middle = part.start + part.len().div_ceil(2);
    (part.len() >= 2).then_some((part.start..middle, middle..part.end))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PartState {
    Requested,
    Saved,
    NotSaved,
    Failed(PartFailure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PartFailure {
    Rerequested,
    TooLarge,
    NoAnswer,
    Contract,
    InvalidName,
    TooManyClashes,
    Discarded,
    NothingLeft,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SaveError {
    Retryable(SaveProblem),
    InvalidName,
    TooManyClashes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PartEvent {
    Saved,
    Cancelled { rerequests: u32 },
    TooLarge { already_split: bool },
    PendingTooLong,
    Expired,
    BadContent,
    SaveFailed(SaveError),
    Discard,
    TryAgain,
    NothingLeft,
}

impl PartState {
    pub const fn apply(self, event: PartEvent) -> Result<Self, IllegalTransition> {
        use PartEvent as E;
        match (self, event) {
            (Self::Requested | Self::NotSaved, E::Saved) => Ok(Self::Saved),
            (Self::Requested, E::Cancelled { rerequests }) => Ok(rerequest(rerequests)),
            (
                Self::Requested,
                E::TooLarge {
                    already_split: true,
                },
            ) => Ok(Self::Failed(PartFailure::TooLarge)),
            (Self::Requested, E::PendingTooLong) => Ok(Self::Failed(PartFailure::NoAnswer)),
            (
                Self::Requested,
                E::Expired
                | E::TooLarge {
                    already_split: false,
                },
            )
            | (Self::Failed(_), E::TryAgain) => Ok(Self::Requested),
            (Self::Requested, E::BadContent) => Ok(Self::Failed(PartFailure::Contract)),
            (Self::Requested, E::NothingLeft) => Ok(Self::Failed(PartFailure::NothingLeft)),
            (Self::Requested | Self::NotSaved, E::SaveFailed(error)) => Ok(save_failed(error)),
            (Self::NotSaved, E::Discard) => Ok(Self::Failed(PartFailure::Discarded)),
            _ => Err(IllegalTransition("report part")),
        }
    }
}

const fn rerequest(rerequests: u32) -> PartState {
    if rerequests < MAX_REREQUESTS {
        PartState::Requested
    } else {
        PartState::Failed(PartFailure::Rerequested)
    }
}

const fn save_failed(error: SaveError) -> PartState {
    match error {
        SaveError::Retryable(_) => PartState::NotSaved,
        SaveError::InvalidName => PartState::Failed(PartFailure::InvalidName),
        SaveError::TooManyClashes => PartState::Failed(PartFailure::TooManyClashes),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReportState {
    Waiting,
    NothingToReport,
    Creating,
    Saved,
    NotSaved,
    Failed,
}

pub fn report_state(computations_pending: bool, parts: &[PartState]) -> ReportState {
    let any = |wanted: fn(&PartState) -> bool| parts.iter().any(wanted);
    if parts.is_empty() {
        return if computations_pending {
            ReportState::Waiting
        } else {
            ReportState::NothingToReport
        };
    }
    if any(|part| matches!(part, PartState::Requested)) {
        return ReportState::Creating;
    }
    if any(|part| matches!(part, PartState::Failed(_))) {
        return ReportState::Failed;
    }
    if any(|part| matches!(part, PartState::NotSaved)) {
        return ReportState::NotSaved;
    }
    ReportState::Saved
}

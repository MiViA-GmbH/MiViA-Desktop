use std::ops::Range;

use crate::transition::IllegalTransition;

pub const MAX_PART_MEMBERS: usize = 100;
pub const MAX_REREQUESTS: u32 = 3;

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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SaveError {
    Retryable,
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
        SaveError::Retryable => PartState::NotSaved,
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

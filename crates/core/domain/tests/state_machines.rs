use mivia_domain::batch::{BatchState, ClosureReason};
use mivia_domain::computation::{ComputationEvent, ComputationState, FailReason, JobOutcome};
use mivia_domain::report::{PartEvent, PartFailure, PartState, SaveError, SaveProblem};
use mivia_domain::source_file::{
    Classification, OpenFailure, RejectReason, SourceFileEvent, SourceFileState, UnreadableReason,
};
use proptest::prelude::*;
use proptest::sample::select;

type Row<S, E> = (S, E, S);

const UNREADABLE: [UnreadableReason; 4] = [
    UnreadableReason::InUse,
    UnreadableReason::Incomplete,
    UnreadableReason::CloudOnly,
    UnreadableReason::AccessDenied,
];
const FAIL: [FailReason; 6] = [
    FailReason::ServerFailed,
    FailReason::NoResult,
    FailReason::Refused,
    FailReason::Contract,
    FailReason::ServerTimeout,
    FailReason::UnknownServerStatus,
];
const PART_FAILURES: [PartFailure; 8] = [
    PartFailure::Rerequested,
    PartFailure::TooLarge,
    PartFailure::NoAnswer,
    PartFailure::Contract,
    PartFailure::InvalidName,
    PartFailure::TooManyClashes,
    PartFailure::Discarded,
    PartFailure::NothingLeft,
];

fn source_states() -> Vec<SourceFileState> {
    use SourceFileState as S;
    let mut states = vec![
        S::Detected,
        S::Ready,
        S::CopyInBatch,
        S::KnownFile,
        S::KnownContent,
        S::Uploading,
        S::Uploaded { image_gone: false },
        S::Uploaded { image_gone: true },
        S::Rejected(RejectReason::TooLarge),
        S::Rejected(RejectReason::Server),
        S::Vanished,
    ];
    states.extend(UNREADABLE.map(S::Unreadable));
    states
}

fn source_events() -> Vec<SourceFileEvent> {
    use SourceFileEvent as E;
    let mut events = vec![
        E::ReadinessPassed,
        E::Gone,
        E::TooLarge,
        E::UploadClaimed,
        E::UploadSucceeded,
        E::UploadRejected,
        E::UploadInterrupted,
        E::ImageGone,
        E::ConfirmReupload,
        E::Removed,
        E::ContentChanged,
    ];
    events.extend(UNREADABLE.map(E::GaveUp));
    events.extend(
        [
            Classification::CopyInBatch,
            Classification::KnownFile,
            Classification::KnownContent,
            Classification::New,
        ]
        .map(E::Classified),
    );
    events.extend(
        [OpenFailure::Gone, OpenFailure::Denied, OpenFailure::Busy].map(E::UploadOpenFailed),
    );
    events
}

fn source_table() -> Vec<Row<SourceFileState, SourceFileEvent>> {
    use SourceFileEvent as E;
    use SourceFileState as S;
    let mut rows = vec![
        (S::Detected, E::ReadinessPassed, S::Ready),
        (S::Detected, E::Gone, S::Vanished),
        (
            S::Ready,
            E::Classified(Classification::CopyInBatch),
            S::CopyInBatch,
        ),
        (
            S::Ready,
            E::Classified(Classification::KnownFile),
            S::KnownFile,
        ),
        (
            S::Ready,
            E::Classified(Classification::KnownContent),
            S::KnownContent,
        ),
        (S::Ready, E::Classified(Classification::New), S::Ready),
        (S::Ready, E::TooLarge, S::Rejected(RejectReason::TooLarge)),
        (S::Ready, E::UploadClaimed, S::Uploading),
        (
            S::Uploading,
            E::UploadSucceeded,
            S::Uploaded { image_gone: false },
        ),
        (
            S::Uploading,
            E::UploadRejected,
            S::Rejected(RejectReason::Server),
        ),
        (
            S::Uploading,
            E::UploadOpenFailed(OpenFailure::Gone),
            S::Vanished,
        ),
        (
            S::Uploading,
            E::UploadOpenFailed(OpenFailure::Denied),
            S::Unreadable(UnreadableReason::AccessDenied),
        ),
        (
            S::Uploading,
            E::UploadOpenFailed(OpenFailure::Busy),
            S::Ready,
        ),
        (S::Uploading, E::UploadInterrupted, S::Ready),
        (
            S::Uploaded { image_gone: false },
            E::ImageGone,
            S::Uploaded { image_gone: true },
        ),
        (
            S::Uploaded { image_gone: true },
            E::ImageGone,
            S::Uploaded { image_gone: true },
        ),
        (
            S::Uploaded { image_gone: true },
            E::ConfirmReupload,
            S::Ready,
        ),
    ];
    rows.extend(UNREADABLE.map(|reason| (S::Detected, E::GaveUp(reason), S::Unreadable(reason))));
    let members = [
        S::Detected,
        S::Ready,
        S::CopyInBatch,
        S::KnownContent,
        S::Uploading,
        S::Uploaded { image_gone: false },
        S::Uploaded { image_gone: true },
    ];
    rows.extend(members.map(|member| (member, E::Removed, S::Vanished)));
    rows.extend(members.map(|member| (member, E::ContentChanged, S::Detected)));
    rows
}

fn computation_states() -> Vec<ComputationState> {
    use ComputationState as S;
    let mut states = vec![
        S::Queued,
        S::Submitting,
        S::Computing,
        S::Done { reused: false },
        S::Done { reused: true },
        S::Unavailable,
        S::Lost,
        S::Cancelled,
        S::Superseded,
    ];
    states.extend(FAIL.map(S::Failed));
    states
}

fn job_outcomes() -> Vec<JobOutcome> {
    let mut outcomes = vec![
        JobOutcome::Pending,
        JobOutcome::Done { reused: false },
        JobOutcome::Done { reused: true },
    ];
    outcomes.extend(FAIL.map(JobOutcome::Failed));
    outcomes
}

fn computation_events() -> Vec<ComputationEvent> {
    use ComputationEvent as E;
    let mut events = vec![
        E::ChoiceUnavailable,
        E::SubmitClaimed,
        E::Ambiguous,
        E::NotSent,
        E::Refused { choice_gone: true },
        E::Refused { choice_gone: false },
        E::ContractError,
        E::JobGone { resubmitted: false },
        E::JobGone { resubmitted: true },
        E::Cancel,
        E::RunAgain,
    ];
    events.extend(job_outcomes().into_iter().map(E::Job));
    events.extend(FAIL.map(E::TimedOut));
    events
}

fn job_target(outcome: JobOutcome) -> ComputationState {
    match outcome {
        JobOutcome::Pending => ComputationState::Computing,
        JobOutcome::Done { reused } => ComputationState::Done { reused },
        JobOutcome::Failed(reason) => ComputationState::Failed(reason),
    }
}

fn computation_table() -> Vec<Row<ComputationState, ComputationEvent>> {
    use ComputationEvent as E;
    use ComputationState as S;
    let mut rows = vec![
        (S::Queued, E::ChoiceUnavailable, S::Unavailable),
        (S::Queued, E::SubmitClaimed, S::Submitting),
        (S::Submitting, E::Ambiguous, S::Submitting),
        (S::Submitting, E::NotSent, S::Queued),
        (
            S::Submitting,
            E::Refused { choice_gone: true },
            S::Unavailable,
        ),
        (
            S::Submitting,
            E::Refused { choice_gone: false },
            S::Failed(FailReason::Refused),
        ),
        (
            S::Submitting,
            E::ContractError,
            S::Failed(FailReason::Contract),
        ),
        (S::Computing, E::JobGone { resubmitted: false }, S::Queued),
        (S::Computing, E::JobGone { resubmitted: true }, S::Lost),
        (S::Done { reused: false }, E::RunAgain, S::Superseded),
        (S::Done { reused: true }, E::RunAgain, S::Superseded),
    ];
    for from in [S::Submitting, S::Computing] {
        rows.extend(
            job_outcomes()
                .into_iter()
                .map(|outcome| (from, E::Job(outcome), job_target(outcome))),
        );
    }
    rows.extend(FAIL.map(|reason| (S::Computing, E::TimedOut(reason), S::Failed(reason))));
    rows.extend(
        [S::Queued, S::Submitting, S::Computing].map(|from| (from, E::Cancel, S::Cancelled)),
    );
    rows
}

fn part_states() -> Vec<PartState> {
    let mut states = vec![PartState::Requested, PartState::Saved, PartState::NotSaved];
    states.extend(PART_FAILURES.map(PartState::Failed));
    states
}

fn part_events() -> Vec<PartEvent> {
    use PartEvent as E;
    let mut events = vec![
        E::Saved,
        E::TooLarge {
            already_split: false,
        },
        E::TooLarge {
            already_split: true,
        },
        E::PendingTooLong,
        E::Expired,
        E::BadContent,
        E::Discard,
        E::TryAgain,
        E::NothingLeft,
    ];
    events.extend((0..5).map(|rerequests| E::Cancelled { rerequests }));
    events.extend(
        [
            SaveError::Retryable(SaveProblem::DiskFull),
            SaveError::InvalidName,
            SaveError::TooManyClashes,
        ]
        .map(E::SaveFailed),
    );
    events
}

fn part_table() -> Vec<Row<PartState, PartEvent>> {
    use PartEvent as E;
    use PartState as S;
    let mut rows = vec![
        (S::Requested, E::Saved, S::Saved),
        (S::NotSaved, E::Saved, S::Saved),
        (
            S::Requested,
            E::TooLarge {
                already_split: false,
            },
            S::Requested,
        ),
        (
            S::Requested,
            E::TooLarge {
                already_split: true,
            },
            S::Failed(PartFailure::TooLarge),
        ),
        (
            S::Requested,
            E::PendingTooLong,
            S::Failed(PartFailure::NoAnswer),
        ),
        (S::Requested, E::Expired, S::Requested),
        (
            S::Requested,
            E::BadContent,
            S::Failed(PartFailure::Contract),
        ),
        (S::NotSaved, E::Discard, S::Failed(PartFailure::Discarded)),
        (
            S::Requested,
            E::NothingLeft,
            S::Failed(PartFailure::NothingLeft),
        ),
    ];
    rows.extend((0..3).map(|rerequests| (S::Requested, E::Cancelled { rerequests }, S::Requested)));
    rows.extend((3..5).map(|rerequests| {
        (
            S::Requested,
            E::Cancelled { rerequests },
            S::Failed(PartFailure::Rerequested),
        )
    }));
    for from in [S::Requested, S::NotSaved] {
        rows.push((
            from,
            E::SaveFailed(SaveError::Retryable(SaveProblem::DiskFull)),
            S::NotSaved,
        ));
        rows.push((
            from,
            E::SaveFailed(SaveError::InvalidName),
            S::Failed(PartFailure::InvalidName),
        ));
        rows.push((
            from,
            E::SaveFailed(SaveError::TooManyClashes),
            S::Failed(PartFailure::TooManyClashes),
        ));
    }
    rows.extend(PART_FAILURES.map(|failure| (S::Failed(failure), E::TryAgain, S::Requested)));
    rows
}

fn expected<S: Copy + PartialEq, E: PartialEq>(
    table: &[Row<S, E>],
    from: S,
    event: &E,
) -> Option<S> {
    table
        .iter()
        .find(|(state, on, _)| *state == from && on == event)
        .map(|(_, _, to)| *to)
}

fn assert_matches_table<S, E>(
    states: &[S],
    events: &[E],
    table: &[Row<S, E>],
    apply: impl Fn(S, E) -> Option<S>,
) where
    S: Copy + PartialEq + std::fmt::Debug,
    E: Copy + PartialEq + std::fmt::Debug,
{
    for (index, (from, event, _)) in table.iter().enumerate() {
        let duplicate = table
            .iter()
            .skip(index + 1)
            .any(|(f, e, _)| f == from && e == event);
        assert!(!duplicate, "duplicate row {from:?} {event:?}");
    }
    for &from in states {
        for &event in events {
            assert_eq!(
                apply(from, event),
                expected(table, from, &event),
                "{from:?} on {event:?}"
            );
        }
    }
}

#[test]
fn source_file_follows_the_transition_table() {
    assert_matches_table(
        &source_states(),
        &source_events(),
        &source_table(),
        |s, e| s.apply(e).ok(),
    );
}

#[test]
fn computation_follows_the_transition_table() {
    assert_matches_table(
        &computation_states(),
        &computation_events(),
        &computation_table(),
        |s, e| s.apply(e).ok(),
    );
}

#[test]
fn report_part_follows_the_transition_table() {
    assert_matches_table(&part_states(), &part_events(), &part_table(), |s, e| {
        s.apply(e).ok()
    });
}

#[test]
fn batch_closes_once() {
    let reasons = [
        ClosureReason::QuietPeriod,
        ClosureReason::Cap,
        ClosureReason::Manual,
        ClosureReason::Restart,
        ClosureReason::AccountSwitch,
        ClosureReason::OneOff,
    ];
    for reason in reasons {
        let closed = BatchState::Open.close(reason);
        assert_eq!(closed, Ok(BatchState::Closed(reason)));
        assert!(
            BatchState::Closed(reason)
                .close(ClosureReason::Manual)
                .is_err()
        );
    }
}

#[test]
fn terminal_flags_match_the_tables() {
    for state in source_states() {
        let leaves = source_events()
            .into_iter()
            .any(|event| state.apply(event).is_ok());
        assert_eq!(state.is_terminal(), !leaves, "{state:?}");
    }
    for state in computation_states() {
        let open = matches!(
            state,
            ComputationState::Queued | ComputationState::Submitting | ComputationState::Computing
        );
        assert_eq!(state.is_terminal(), !open, "{state:?}");
    }
}

fn walk<S: Copy, E: Copy>(
    start: S,
    events: &[E],
    apply: impl Fn(S, E) -> Option<S>,
) -> Vec<(S, E, Option<S>)> {
    let mut state = start;
    events
        .iter()
        .map(|&event| {
            let next = apply(state, event);
            let step = (state, event, next);
            state = next.unwrap_or(state);
            step
        })
        .collect()
}

proptest! {
    #[test]
    fn terminal_computations_are_left_only_by_run_again(events in prop::collection::vec(select(computation_events()), 0..40)) {
        for (from, event, next) in walk(ComputationState::Queued, &events, |s, e| s.apply(e).ok()) {
            if from.is_terminal() && next.is_some() {
                let done = matches!(from, ComputationState::Done { .. });
                prop_assert!(done);
                prop_assert_eq!(event, ComputationEvent::RunAgain);
            }
        }
    }

    #[test]
    fn terminal_source_files_never_change(events in prop::collection::vec(select(source_events()), 0..40)) {
        for (from, _, next) in walk(SourceFileState::Detected, &events, |s, e| s.apply(e).ok()) {
            prop_assert!(!(from.is_terminal() && next.is_some()));
        }
    }

    #[test]
    fn failed_parts_are_left_only_by_try_again(events in prop::collection::vec(select(part_events()), 0..40)) {
        for (from, event, next) in walk(PartState::Requested, &events, |s, e| s.apply(e).ok()) {
            if matches!(from, PartState::Failed(_)) && next.is_some() {
                prop_assert_eq!(event, PartEvent::TryAgain);
            }
            if from == PartState::Saved {
                prop_assert!(next.is_none());
            }
        }
    }
}

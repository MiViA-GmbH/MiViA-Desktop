use mivia_domain::batch::ClosureReason;
use mivia_domain::computation::{ComputationState, FailReason};
use mivia_domain::hold::{HoldReason, KeyProblem, LicenseScope, NetworkBlock};
use mivia_domain::label::LabelRule;
use mivia_domain::report::{PartFailure, PartState};
use mivia_domain::source_file::{RejectReason, SourceFileState, UnreadableReason};
use mivia_ports::store::TxError;

pub(crate) type Coded = (&'static str, Option<&'static str>);

fn corrupt(what: &str, state: &str, reason: Option<&str>) -> TxError {
    TxError::Corrupt(format!("{what} {state}/{}", reason.unwrap_or("-")))
}

pub(crate) const fn hold_reason(reason: HoldReason) -> &'static str {
    match reason {
        HoldReason::Offline => "offline",
        HoldReason::ServiceUnavailable => "service_unavailable",
        HoldReason::NetworkBlocked(NetworkBlock::Tls) => "network_blocked.tls",
        HoldReason::NetworkBlocked(NetworkBlock::ProxyAuth) => "network_blocked.proxy_auth",
        HoldReason::NetworkBlocked(NetworkBlock::ProxyRefused) => "network_blocked.proxy_refused",
        HoldReason::NetworkBlocked(NetworkBlock::Redirect) => "network_blocked.redirect",
        HoldReason::AccessKeyInvalid(KeyProblem::Invalid) => "access_key_invalid.invalid",
        HoldReason::AccessKeyInvalid(KeyProblem::AccountBlocked) => {
            "access_key_invalid.account_blocked"
        }
        HoldReason::AccessKeyInvalid(KeyProblem::AdminScope) => "access_key_invalid.admin_scope",
        HoldReason::LicenseInvalid(LicenseScope::Uploads) => "license_invalid.uploads",
        HoldReason::LicenseInvalid(LicenseScope::All) => "license_invalid.all",
        HoldReason::ClientTooOld => "client_too_old",
    }
}

pub(crate) const HOLD_REASONS: [HoldReason; 12] = [
    HoldReason::Offline,
    HoldReason::ServiceUnavailable,
    HoldReason::NetworkBlocked(NetworkBlock::Tls),
    HoldReason::NetworkBlocked(NetworkBlock::ProxyAuth),
    HoldReason::NetworkBlocked(NetworkBlock::ProxyRefused),
    HoldReason::NetworkBlocked(NetworkBlock::Redirect),
    HoldReason::AccessKeyInvalid(KeyProblem::Invalid),
    HoldReason::AccessKeyInvalid(KeyProblem::AccountBlocked),
    HoldReason::AccessKeyInvalid(KeyProblem::AdminScope),
    HoldReason::LicenseInvalid(LicenseScope::Uploads),
    HoldReason::LicenseInvalid(LicenseScope::All),
    HoldReason::ClientTooOld,
];

pub(crate) fn parse_hold_reason(text: &str) -> Result<HoldReason, TxError> {
    HOLD_REASONS
        .into_iter()
        .find(|reason| hold_reason(*reason) == text)
        .ok_or_else(|| corrupt("hold", text, None))
}

pub(crate) const fn source_state(state: SourceFileState) -> Coded {
    use SourceFileState as S;
    match state {
        S::Detected => ("detected", None),
        S::Ready => ("ready", None),
        S::CopyInBatch => ("copy_in_batch", None),
        S::KnownFile => ("known_file", None),
        S::KnownContent => ("known_content", None),
        S::Uploading => ("uploading", None),
        S::Uploaded { image_gone: false } => ("uploaded", None),
        S::Uploaded { image_gone: true } => ("uploaded", Some("image_gone")),
        S::Rejected(RejectReason::TooLarge) => ("rejected", Some("too_large")),
        S::Rejected(RejectReason::Server) => ("rejected", Some("server")),
        S::Vanished => ("vanished", None),
        S::Unreadable(UnreadableReason::InUse) => ("unreadable", Some("in_use")),
        S::Unreadable(UnreadableReason::Incomplete) => ("unreadable", Some("incomplete")),
        S::Unreadable(UnreadableReason::CloudOnly) => ("unreadable", Some("cloud_only")),
        S::Unreadable(UnreadableReason::AccessDenied) => ("unreadable", Some("access_denied")),
    }
}

pub(crate) const fn computation_state(state: ComputationState) -> Coded {
    use ComputationState as S;
    match state {
        S::Queued => ("queued", None),
        S::Submitting => ("submitting", None),
        S::Computing => ("computing", None),
        S::Done { reused: false } => ("done", None),
        S::Done { reused: true } => ("done", Some("reused")),
        S::Failed(reason) => ("failed", Some(fail_reason(reason))),
        S::Unavailable => ("unavailable", None),
        S::Lost => ("lost", None),
        S::Cancelled => ("cancelled", None),
        S::Superseded => ("superseded", None),
    }
}

const fn fail_reason(reason: FailReason) -> &'static str {
    match reason {
        FailReason::ServerFailed => "server_failed",
        FailReason::NoResult => "no_result",
        FailReason::Refused => "refused",
        FailReason::Contract => "contract",
        FailReason::ServerTimeout => "server_timeout",
        FailReason::UnknownServerStatus => "unknown_server_status",
    }
}

pub(crate) const fn part_state(state: PartState) -> Coded {
    match state {
        PartState::Requested => ("requested", None),
        PartState::Saved => ("saved", None),
        PartState::NotSaved => ("not_saved", None),
        PartState::Failed(failure) => ("failed", Some(part_failure(failure))),
    }
}

const fn part_failure(failure: PartFailure) -> &'static str {
    match failure {
        PartFailure::Rerequested => "rerequested",
        PartFailure::TooLarge => "too_large",
        PartFailure::NoAnswer => "no_answer",
        PartFailure::Contract => "contract",
        PartFailure::InvalidName => "invalid_name",
        PartFailure::TooManyClashes => "too_many_clashes",
        PartFailure::Discarded => "discarded",
    }
}

pub(crate) fn decode<T: Copy>(
    what: &str,
    all: &[T],
    encode: fn(T) -> Coded,
    stored: (&str, Option<&str>),
) -> Result<T, TxError> {
    all.iter()
        .copied()
        .find(|candidate| encode(*candidate) == stored)
        .ok_or_else(|| corrupt(what, stored.0, stored.1))
}

pub(crate) const fn closure_reason(reason: ClosureReason) -> &'static str {
    match reason {
        ClosureReason::QuietPeriod => "quiet_period",
        ClosureReason::Cap => "cap",
        ClosureReason::Manual => "manual",
        ClosureReason::Restart => "restart",
        ClosureReason::AccountSwitch => "account_switch",
        ClosureReason::OneOff => "one_off",
    }
}

pub(crate) const fn label_rule(rule: LabelRule) -> &'static str {
    match rule {
        LabelRule::Auto => "auto",
        LabelRule::Range => "range",
        LabelRule::Folder => "folder",
    }
}

pub(crate) const SOURCE_STATES: [SourceFileState; 15] = [
    SourceFileState::Detected,
    SourceFileState::Ready,
    SourceFileState::CopyInBatch,
    SourceFileState::KnownFile,
    SourceFileState::KnownContent,
    SourceFileState::Uploading,
    SourceFileState::Uploaded { image_gone: false },
    SourceFileState::Uploaded { image_gone: true },
    SourceFileState::Rejected(RejectReason::TooLarge),
    SourceFileState::Rejected(RejectReason::Server),
    SourceFileState::Vanished,
    SourceFileState::Unreadable(UnreadableReason::InUse),
    SourceFileState::Unreadable(UnreadableReason::Incomplete),
    SourceFileState::Unreadable(UnreadableReason::CloudOnly),
    SourceFileState::Unreadable(UnreadableReason::AccessDenied),
];

pub(crate) const COMPUTATION_STATES: [ComputationState; 15] = [
    ComputationState::Queued,
    ComputationState::Submitting,
    ComputationState::Computing,
    ComputationState::Done { reused: false },
    ComputationState::Done { reused: true },
    ComputationState::Failed(FailReason::ServerFailed),
    ComputationState::Failed(FailReason::NoResult),
    ComputationState::Failed(FailReason::Refused),
    ComputationState::Failed(FailReason::Contract),
    ComputationState::Failed(FailReason::ServerTimeout),
    ComputationState::Failed(FailReason::UnknownServerStatus),
    ComputationState::Unavailable,
    ComputationState::Lost,
    ComputationState::Cancelled,
    ComputationState::Superseded,
];

pub(crate) const PART_STATES: [PartState; 10] = [
    PartState::Requested,
    PartState::Saved,
    PartState::NotSaved,
    PartState::Failed(PartFailure::Rerequested),
    PartState::Failed(PartFailure::TooLarge),
    PartState::Failed(PartFailure::NoAnswer),
    PartState::Failed(PartFailure::Contract),
    PartState::Failed(PartFailure::InvalidName),
    PartState::Failed(PartFailure::TooManyClashes),
    PartState::Failed(PartFailure::Discarded),
];

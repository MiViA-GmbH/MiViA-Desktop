use crate::ids::{MINUTE_MS, SECOND_MS};
use crate::retry::RetryPolicy;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NetworkBlock {
    Tls,
    ProxyAuth,
    ProxyRefused,
    Redirect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum KeyProblem {
    Invalid,
    AccountBlocked,
    AdminScope,
    MissingScopes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LicenseScope {
    Uploads,
    All,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HoldReason {
    Offline,
    ServiceUnavailable,
    NetworkBlocked(NetworkBlock),
    AccessKeyInvalid(KeyProblem),
    LicenseInvalid(LicenseScope),
    ClientTooOld,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Gate {
    Upload,
    Submit,
    StatusPoll,
    ReportRequest,
    ReportDownload,
    Catalogue,
    Labels,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Access {
    Allowed,
    Throttled { min_interval_ms: i64 },
    Blocked,
}

pub const SERVICE_UNAVAILABLE_POLL_MS: i64 = 60 * SECOND_MS;

impl HoldReason {
    pub const fn access(self, gate: Gate) -> Access {
        match self {
            Self::Offline | Self::NetworkBlocked(_) => Access::Blocked,
            Self::ServiceUnavailable => service_unavailable(gate),
            Self::AccessKeyInvalid(_) => blocked_if(!matches!(gate, Gate::Labels)),
            Self::LicenseInvalid(LicenseScope::Uploads) | Self::ClientTooOld => {
                blocked_if(matches!(gate, Gate::Upload))
            }
            Self::LicenseInvalid(LicenseScope::All) => {
                blocked_if(matches!(gate, Gate::Upload | Gate::Submit))
            }
        }
    }

    pub const fn probe(self) -> Option<RetryPolicy> {
        match self {
            Self::Offline => Some(RetryPolicy::probe(15 * SECOND_MS, 5 * MINUTE_MS)),
            Self::ServiceUnavailable => Some(RetryPolicy::probe(30 * SECOND_MS, 5 * MINUTE_MS)),
            Self::NetworkBlocked(_) => Some(RetryPolicy::probe(MINUTE_MS, 15 * MINUTE_MS)),
            Self::AccessKeyInvalid(KeyProblem::MissingScopes) | Self::ClientTooOld => None,
            Self::AccessKeyInvalid(_) => Some(RetryPolicy::probe(30 * MINUTE_MS, 30 * MINUTE_MS)),
            Self::LicenseInvalid(_) => Some(RetryPolicy::LICENSE_CANARY),
        }
    }
}

pub fn can_claim(gate: Gate, holds: &[HoldReason]) -> Access {
    holds
        .iter()
        .map(|hold| hold.access(gate))
        .max()
        .unwrap_or(Access::Allowed)
}

const fn service_unavailable(gate: Gate) -> Access {
    match gate {
        Gate::StatusPoll => Access::Throttled {
            min_interval_ms: SERVICE_UNAVAILABLE_POLL_MS,
        },
        Gate::ReportDownload | Gate::Catalogue | Gate::Labels => Access::Allowed,
        Gate::Upload | Gate::Submit | Gate::ReportRequest => Access::Blocked,
    }
}

const fn blocked_if(blocked: bool) -> Access {
    if blocked {
        Access::Blocked
    } else {
        Access::Allowed
    }
}

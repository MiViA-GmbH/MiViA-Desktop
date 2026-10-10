use mivia_domain::ids::Timestamp;
use mivia_domain::update::{Offer, Refusal, USER_IDLE_MS, parse};
use mivia_ports::ipc::update::UpdateState;
use mivia_ports::telemetry::{
    CheckState, CheckTrigger, DeferReason, UpdateApplied, UpdateCheck, UpdateFailed, UpdateFailure,
    UpdateStage, Version,
};
use mivia_ports::update::{Activity, UpdateError};

use super::settings::PendingInstall;
use super::{Trigger, Updates};
pub fn telemetry_version(text: &str) -> Option<Version> {
    let [major, minor, patch] = parse(text)?.core();
    Some(Version {
        major: u32::try_from(major).ok()?,
        minor: u32::try_from(minor).ok()?,
        patch: u32::try_from(patch).ok()?,
    })
}

const fn failure(error: UpdateError) -> (UpdateStage, UpdateFailure) {
    match error {
        UpdateError::Network => (UpdateStage::Download, UpdateFailure::Network),
        UpdateError::Disk => (UpdateStage::Download, UpdateFailure::Disk),
        UpdateError::Install => (UpdateStage::Install, UpdateFailure::Other),
        UpdateError::Refused(Refusal::Signature | Refusal::SignedVersion) => {
            (UpdateStage::Signature, UpdateFailure::Verify)
        }
        UpdateError::Refused(Refusal::Authenticode) => {
            (UpdateStage::Authenticode, UpdateFailure::Verify)
        }
        UpdateError::Refused(Refusal::TooLarge) => (UpdateStage::Download, UpdateFailure::Verify),
        UpdateError::Refused(
            Refusal::Prerelease | Refusal::UrlPrefix | Refusal::Blocked | Refusal::Downgrade,
        ) => (UpdateStage::Manifest, UpdateFailure::Verify),
    }
}

pub(super) fn defer_reason(activity: &Activity) -> Option<DeferReason> {
    if let Some(reason) = activity.busy.first() {
        Some(*reason)
    } else if activity.user_idle_ms < USER_IDLE_MS {
        Some(DeferReason::UserActive)
    } else if !activity.accepts_notifications {
        Some(DeferReason::NotificationsOff)
    } else {
        None
    }
}

impl Updates {
    pub(super) fn settled(&self, pending: &PendingInstall, now: Timestamp) {
        let from = telemetry_version(&pending.from);
        let to = telemetry_version(&pending.to);
        if pending.from == self.config.current {
            tracing::error!(from = %pending.from, to = %pending.to, "update.installFailed");
            self.ports.telemetry.record(
                UpdateFailed {
                    stage: Some(UpdateStage::Install),
                    from,
                    to,
                    kind: Some(UpdateFailure::Other),
                }
                .into(),
            );
        } else {
            tracing::info!(from = %pending.from, to = %pending.to, "update.applied");
            self.ports.telemetry.record(
                UpdateApplied {
                    from,
                    to,
                    duration_ms: u64::try_from(now.ms_since(Timestamp(pending.at))).ok(),
                }
                .into(),
            );
        }
    }

    pub(super) fn record_check(&self, state: CheckState, offer: Option<&Offer>, trigger: Trigger) {
        self.ports.telemetry.record(
            UpdateCheck {
                state: Some(state),
                to: offer.and_then(|offer| telemetry_version(&offer.version)),
                trigger: Some(match trigger {
                    Trigger::Start => CheckTrigger::Start,
                    Trigger::Schedule => CheckTrigger::Schedule,
                    Trigger::Manual => CheckTrigger::Manual,
                }),
            }
            .into(),
        );
    }

    pub(super) fn failed(&self, error: UpdateError, to: Option<&str>) {
        tracing::error!(?error, to, "update.failed");
        let (stage, kind) = failure(error);
        self.ports.telemetry.record(
            UpdateFailed {
                stage: Some(stage),
                from: telemetry_version(&self.config.current),
                to: to.and_then(telemetry_version),
                kind: Some(kind),
            }
            .into(),
        );
        self.inner.lock().state = UpdateState::Failed;
    }
}

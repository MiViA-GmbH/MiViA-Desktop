use mivia_domain::hold::{HoldReason, LicenseScope, NetworkBlock};
use mivia_domain::ids::{MINUTE_MS, Timestamp};
use mivia_domain::retry::RetryPolicy;
use mivia_ports::server::ServerError;
use mivia_ports::store::pipeline::Retry;

use super::{Pipeline, PipelineError};

const QUIET_RETRY_MS: i64 = MINUTE_MS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Call {
    Upload,
    Submit,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Item {
    pub(super) name: String,
    pub(super) attempts: u32,
}

impl Item {
    pub(super) fn new(name: impl Into<String>, attempts: u32) -> Self {
        Self {
            name: name.into(),
            attempts,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Effect {
    Hold(HoldReason),
    Unreachable,
    Transient,
    RateLimited(i64),
    VersionCheck,
    Quiet,
    Item,
}

const fn license(call: Call) -> Effect {
    match call {
        Call::Upload => Effect::Hold(HoldReason::LicenseInvalid(LicenseScope::Uploads)),
        Call::Submit => Effect::Hold(HoldReason::LicenseInvalid(LicenseScope::All)),
        Call::Other => Effect::Item,
    }
}

fn effect(error: &ServerError, call: Call) -> Effect {
    use ServerError as E;
    match error {
        E::Unreachable => Effect::Unreachable,
        E::TlsUntrusted => Effect::Hold(HoldReason::NetworkBlocked(NetworkBlock::Tls)),
        E::ProxyAuth => Effect::Hold(HoldReason::NetworkBlocked(NetworkBlock::ProxyAuth)),
        E::ProxyRefused => Effect::Hold(HoldReason::NetworkBlocked(NetworkBlock::ProxyRefused)),
        E::Redirect => Effect::Hold(HoldReason::NetworkBlocked(NetworkBlock::Redirect)),
        E::Unauthorized(problem) => Effect::Hold(HoldReason::AccessKeyInvalid(*problem)),
        E::LicenseInvalid => license(call),
        E::AfterSend | E::StallTimeout | E::Unavailable { .. } => Effect::Transient,
        E::RateLimited { retry_after_ms } => Effect::RateLimited(*retry_after_ms),
        E::Gone => Effect::VersionCheck,
        E::NotSignedIn | E::AccountMismatch => Effect::Quiet,
        E::Forbidden { .. }
        | E::ImageRejected { .. }
        | E::TooLarge { .. }
        | E::NotFound { .. }
        | E::BadRequest { .. }
        | E::InProgress
        | E::Contract
        | E::Source(_) => Effect::Item,
    }
}

const fn uncounted(at: Timestamp, attempts: u32) -> Retry {
    Retry { at, attempts }
}

impl Pipeline {
    fn queue_retry(&self, attempts: u32) -> Retry {
        let attempts = if self.counts_attempt() {
            attempts.saturating_add(1)
        } else {
            attempts
        };
        let delay = RetryPolicy::QUEUE.delay_ms(attempts.max(1), self.random());
        Retry {
            at: self.now().plus_ms(delay),
            attempts,
        }
    }

    pub(super) async fn transient(
        &self,
        item: String,
        attempts: u32,
    ) -> Result<Retry, PipelineError> {
        if self.note_transient(item) {
            self.service_check().await?;
        }
        Ok(self.queue_retry(attempts))
    }

    pub(super) async fn infrastructure(
        &self,
        error: &ServerError,
        call: Call,
        item: Item,
    ) -> Result<Option<Retry>, PipelineError> {
        let Item { name, attempts } = item;
        let now = self.now();
        let retry = match effect(error, call) {
            Effect::Item => return Ok(None),
            Effect::Hold(reason) => {
                self.raise_hold(reason).await?;
                uncounted(now, attempts)
            }
            Effect::Unreachable if self.confirm_offline().await? => uncounted(now, attempts),
            Effect::Unreachable => self.queue_retry(attempts),
            Effect::Transient => self.transient(name, attempts).await?,
            Effect::RateLimited(ms) => uncounted(now.plus_ms(ms), attempts),
            Effect::VersionCheck => {
                let version = self.ports.server.version().await;
                tracing::warn!(ok = version.is_ok(), "api.notFoundVersionCheck");
                self.queue_retry(attempts)
            }
            Effect::Quiet => uncounted(now.plus_ms(QUIET_RETRY_MS), attempts),
        };
        tracing::debug!(?error, ?call, at = retry.at.0, "pipeline.retry");
        Ok(Some(retry))
    }
}

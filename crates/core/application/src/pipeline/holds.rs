use std::collections::{BTreeMap, BTreeSet, VecDeque};

use mivia_domain::hold::HoldReason;
use mivia_domain::ids::{MINUTE_MS, SECOND_MS, Timestamp};
use mivia_domain::retry::RetryPolicy;
use parking_lot::Mutex;

use super::{Pipeline, PipelineError};

const FLAP_WINDOW_MS: i64 = 2 * MINUTE_MS;
const UNCOUNTED_AFTER_LIFT_MS: i64 = MINUTE_MS;
const TRANSIENT_WINDOW_MS: i64 = 2 * MINUTE_MS;
const TRANSIENTS_FOR_SERVICE: usize = 5;
const ITEMS_FOR_SERVICE: usize = 3;
const OFFLINE_CONFIRM_GAP_MS: i64 = 5 * SECOND_MS;

#[derive(Default)]
pub(super) struct HoldState {
    attempts: Mutex<BTreeMap<HoldReason, u32>>,
    lifted: Mutex<BTreeMap<HoldReason, (Timestamp, u32)>>,
    last_lift: Mutex<Option<Timestamp>>,
    transients: Mutex<VecDeque<(Timestamp, String)>>,
    offline_check: tokio::sync::Mutex<()>,
}

fn policy(reason: HoldReason) -> Option<RetryPolicy> {
    reason.probe()
}

impl Pipeline {
    fn probe_at(&self, reason: HoldReason, attempt: u32) -> Option<Timestamp> {
        let delay = policy(reason)?.delay_ms(attempt, self.random());
        Some(self.now().plus_ms(delay))
    }

    pub(super) async fn raise_hold(&self, reason: HoldReason) -> Result<(), PipelineError> {
        let now = self.now();
        let flapped = self
            .holds
            .lifted
            .lock()
            .get(&reason)
            .filter(|(at, _)| now.ms_since(*at) < FLAP_WINDOW_MS)
            .map(|(_, attempts)| *attempts);
        let attempt = flapped.unwrap_or(1);
        self.holds.attempts.lock().insert(reason, attempt);
        let next = self.probe_at(reason, attempt);
        if let Some(at) = next {
            self.due_later("probe", at);
        }
        self.write(move |tx, scope| tx.holds(scope).raise(reason, now, next))
            .await?;
        tracing::info!(?reason, "hold.raised");
        self.wakers.probe.notify_one();
        Ok(())
    }

    pub(super) async fn reschedule_probe(&self, reason: HoldReason) -> Result<(), PipelineError> {
        let attempt = {
            let mut attempts = self.holds.attempts.lock();
            let attempt = attempts.entry(reason).or_insert(1);
            *attempt = attempt.saturating_add(1);
            *attempt
        };
        let now = self.now();
        let next = self.probe_at(reason, attempt);
        if let Some(at) = next {
            self.due_later("probe", at);
        }
        self.write(move |tx, scope| tx.holds(scope).raise(reason, now, next))
            .await
    }

    pub(super) async fn lift_hold(&self, reason: HoldReason) -> Result<(), PipelineError> {
        let now = self.now();
        self.write(move |tx, scope| tx.holds(scope).release(reason))
            .await?;
        let attempts = self.holds.attempts.lock().remove(&reason).unwrap_or(1);
        self.holds.lifted.lock().insert(reason, (now, attempts));
        *self.holds.last_lift.lock() = Some(now);
        tracing::info!(?reason, "hold.lifted");
        self.wakers.all();
        Ok(())
    }

    pub(super) fn counts_attempt(&self) -> bool {
        let now = self.now();
        self.holds
            .last_lift
            .lock()
            .is_none_or(|at| now.ms_since(at) >= UNCOUNTED_AFTER_LIFT_MS)
    }

    pub(super) fn release_budget(&self, permits: usize) -> usize {
        let now = self.now();
        let Some(lifted) = *self.holds.last_lift.lock() else {
            return permits;
        };
        let seconds = usize::try_from(now.ms_since(lifted) / SECOND_MS).unwrap_or(usize::MAX);
        permits.min(1_usize.saturating_add(seconds)).max(1)
    }

    pub(super) fn note_transient(&self, item: String) -> bool {
        let now = self.now();
        let mut window = self.holds.transients.lock();
        window.push_back((now, item));
        while window
            .front()
            .is_some_and(|(at, _)| now.ms_since(*at) > TRANSIENT_WINDOW_MS)
        {
            window.pop_front();
        }
        let items: BTreeSet<&String> = window.iter().map(|(_, item)| item).collect();
        window.len() >= TRANSIENTS_FOR_SERVICE && items.len() >= ITEMS_FOR_SERVICE
    }

    pub(super) async fn service_check(&self) -> Result<(), PipelineError> {
        if self.ports.server.version().await.is_ok() {
            self.holds.transients.lock().clear();
            self.raise_hold(HoldReason::ServiceUnavailable).await?;
        }
        Ok(())
    }

    pub(super) async fn confirm_offline(&self) -> Result<bool, PipelineError> {
        let _single = self.holds.offline_check.lock().await;
        if self.active_holds().await?.contains(&HoldReason::Offline) {
            return Ok(true);
        }
        if self.ports.server.version().await.is_ok() {
            return Ok(false);
        }
        let again = self.now().plus_ms(OFFLINE_CONFIRM_GAP_MS);
        self.ports.clock.sleep_until(again).await;
        if self.ports.server.version().await.is_ok() {
            return Ok(false);
        }
        self.raise_hold(HoldReason::Offline).await?;
        Ok(true)
    }
}

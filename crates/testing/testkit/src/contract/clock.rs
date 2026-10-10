use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use mivia_ports::clock::Clock;

use super::{CaseResult, check, check_eq};

#[async_trait]
pub trait ClockHarness: Send + Sync {
    fn sut(&self) -> Arc<dyn Clock>;
    async fn advance(&self, ms: i64);
}

const WAIT_LIMIT: Duration = Duration::from_secs(10);

pub async fn clock_past_deadline_returns_at_once(harness: &impl ClockHarness) -> CaseResult {
    let clock = harness.sut();
    let past = clock.now().plus_ms(-1);
    tokio::time::timeout(WAIT_LIMIT, clock.sleep_until(past))
        .await
        .map_err(|_| "sleep_until in the past blocked".to_owned())
}

pub async fn clock_deadline_completes_after_advance(harness: &impl ClockHarness) -> CaseResult {
    let clock = harness.sut();
    let deadline = clock.now().plus_ms(50);
    let sleeper = tokio::spawn(async move { clock.sleep_until(deadline).await });
    tokio::task::yield_now().await;
    check(
        !sleeper.is_finished(),
        "sleep_until returned before the deadline",
    )?;
    harness.advance(50).await;
    tokio::time::timeout(WAIT_LIMIT, sleeper)
        .await
        .map_err(|_| "sleep_until did not wake after the deadline".to_owned())?
        .map_err(|error| error.to_string())
}

pub async fn clock_now_moves_with_advance(harness: &impl ClockHarness) -> CaseResult {
    let clock = harness.sut();
    let before = clock.now();
    harness.advance(20).await;
    let moved = clock.now().ms_since(before);
    check_eq(&(moved >= 20), &true, "elapsed after advance")
}

#[macro_export]
macro_rules! clock_contract_tests {
    ($harness:expr) => {
        $crate::contract_cases!(
            clock,
            $harness,
            clock_past_deadline_returns_at_once,
            clock_deadline_completes_after_advance,
            clock_now_moves_with_advance,
        );
    };
}

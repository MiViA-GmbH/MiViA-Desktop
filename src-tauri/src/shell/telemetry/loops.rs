use std::sync::Arc;
use std::time::{Duration, Instant};

use mivia_diagnostics::{Conditions, Flush, Step};
use mivia_domain::hold::HoldReason;
use mivia_ports::store::StoreExt;
use tauri::async_runtime::{JoinHandle, spawn};

use super::events::{alive, begin, snapshot};
use super::{Link, Telemetry};
use crate::shell::cx::{Services, now};

const WRITE_EVERY: Duration = Duration::from_secs(1);
const SEND_EVERY: Duration = Duration::from_secs(5);
const ALIVE_EVERY: Duration = Duration::from_hours(1);
const DAILY: Duration = Duration::from_hours(24);

const fn stops_sending(hold: HoldReason) -> bool {
    matches!(
        hold,
        HoldReason::Offline | HoldReason::NetworkBlocked(_) | HoldReason::AccessKeyInvalid(_)
    )
}

async fn blocked(services: &Services, link: &Link) -> bool {
    let scope = link.scope;
    let held = services
        .store
        .write(move |tx| tx.holds(scope).active())
        .await;
    held.map_or(true, |holds| holds.iter().copied().any(stops_sending))
}

pub(super) async fn step(services: &Services, link: &Link, flush: Flush) -> Step {
    let telemetry: &Telemetry = &services.telemetry;
    let open = telemetry.sink.enabled() && link.completed && !blocked(services, link).await;
    let account = link.account();
    let machine = || telemetry.machine_id();
    let conditions = Conditions {
        api: link.api.as_ref(),
        store: services.store.as_ref(),
        now: now().0,
        open,
        uploading: link.pipeline.busy(),
        install_id: &services.diagnostics.install_id,
        machine_id: &machine,
        account: Some(&account),
    };
    telemetry.sender.step(flush, &conditions).await
}

pub(crate) fn spawn_sender(services: Arc<Services>) -> JoinHandle<()> {
    spawn(async move {
        loop {
            tokio::time::sleep(SEND_EVERY).await;
            let Some(link) = services.telemetry.linked() else {
                continue;
            };
            match step(&services, &link, Flush::Regular).await {
                Step::Sent { events } => tracing::info!(events, "telemetry.sent"),
                Step::Failed { retry_at } => tracing::warn!(retry_at, "telemetry.retryLater"),
                Step::Idle | Step::Stopped => {}
            }
        }
    })
}

async fn persist(services: &Services) {
    let sink = &services.telemetry.sink;
    if let Err(error) = sink.persist(services.store.as_ref()).await {
        tracing::warn!(?error, "telemetry.persistFailed");
    }
}

async fn daily(services: &Services) {
    snapshot(services).await;
    let kept = services
        .telemetry
        .sink
        .maintain(services.store.as_ref())
        .await;
    if let Err(error) = kept {
        tracing::warn!(?error, "telemetry.maintainFailed");
    }
}

pub(crate) fn spawn_writer(services: Arc<Services>) {
    spawn(async move {
        begin(&services).await;
        snapshot(&services).await;
        let mut alive_at = Instant::now();
        let mut daily_at = Instant::now();
        loop {
            persist(&services).await;
            if alive_at.elapsed() >= ALIVE_EVERY {
                alive_at = Instant::now();
                alive(&services).await;
            }
            if daily_at.elapsed() >= DAILY {
                daily_at = Instant::now();
                daily(&services).await;
            }
            let wake = &services.telemetry.wake;
            if services.telemetry.sink.stats().buffered == 0 {
                tokio::select! {
                    () = wake.notified() => {}
                    () = tokio::time::sleep(ALIVE_EVERY.saturating_sub(alive_at.elapsed())) => {}
                }
            }
            tokio::select! {
                () = wake.notified() => {}
                () = tokio::time::sleep(WRITE_EVERY) => {}
            }
        }
    });
}

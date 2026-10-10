use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use mivia_application::intake::{IntakeError, Tick, run};
use mivia_ports::fs::FsError;
use mivia_testkit::clock::ManualClock;
use tokio::sync::Notify;

use super::world::START;

async fn spin() {
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn the_worker_loop_drains_sleeps_until_due_and_wakes_on_notify() {
    let clock = Arc::new(ManualClock::new(START));
    let wake = Arc::new(Notify::new());
    let ticks = Arc::new(AtomicU32::new(0));
    let worker = {
        let (clock, wake, ticks) = (Arc::clone(&clock), Arc::clone(&wake), Arc::clone(&ticks));
        tokio::spawn(async move {
            run("test", clock.as_ref(), &wake, || {
                let n = ticks.fetch_add(1, Ordering::SeqCst) + 1;
                async move {
                    match n {
                        1 => Ok(Tick::Busy),
                        2 => Ok(Tick::DueAt(START.plus_ms(1_500))),
                        3 => Err(IntakeError::Fs(FsError::Io)),
                        _ => Ok(Tick::Idle),
                    }
                }
            })
            .await;
        })
    };
    spin().await;
    assert_eq!(ticks.load(Ordering::SeqCst), 2);
    assert_eq!(clock.pending_waiters(), [START.plus_ms(2_000)]);
    clock.advance(2_000);
    spin().await;
    assert_eq!(ticks.load(Ordering::SeqCst), 3);
    clock.advance(5_000);
    spin().await;
    assert_eq!(ticks.load(Ordering::SeqCst), 4);
    wake.notify_one();
    spin().await;
    assert_eq!(ticks.load(Ordering::SeqCst), 5);
    worker.abort();
}

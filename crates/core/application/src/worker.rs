use mivia_domain::ids::{SECOND_MS, Timestamp};
use mivia_ports::clock::Clock;
use tokio::sync::Notify;

const MAX_SLEEP_MS: i64 = 60 * SECOND_MS;
const ERROR_PAUSE_MS: i64 = 5 * SECOND_MS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tick {
    Busy,
    Idle,
    DueAt(Timestamp),
}

fn on_grid(at: Timestamp) -> Timestamp {
    Timestamp(at.0.div_euclid(SECOND_MS).saturating_add(1) * SECOND_MS)
}

pub async fn run<F, Fut, E>(name: &str, clock: &dyn Clock, wake: &Notify, mut tick: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<Tick, E>>,
    E: std::fmt::Debug,
{
    loop {
        let next = match tick().await {
            Ok(Tick::Busy) => continue,
            Ok(Tick::Idle) => None,
            Ok(Tick::DueAt(at)) => Some(on_grid(at)),
            Err(error) => {
                tracing::warn!(worker = name, ?error, "worker.tickFailed");
                Some(clock.now().plus_ms(ERROR_PAUSE_MS))
            }
        };
        let latest = clock.now().plus_ms(MAX_SLEEP_MS);
        let deadline = next.map_or(latest, |at| at.min(latest));
        tokio::select! {
            () = wake.notified() => {}
            () = clock.sleep_until(deadline) => {}
        }
    }
}

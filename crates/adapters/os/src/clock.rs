use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use mivia_domain::ids::Timestamp;
use mivia_ports::clock::Clock;

#[derive(Default)]
pub struct SystemClock;

#[async_trait]
impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        let since = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        Timestamp(i64::try_from(since.as_millis()).unwrap_or(i64::MAX))
    }

    async fn sleep_until(&self, deadline: Timestamp) {
        let wait = u64::try_from(deadline.ms_since(self.now())).unwrap_or_default();
        if wait > 0 {
            tokio::time::sleep(Duration::from_millis(wait)).await;
        }
    }
}

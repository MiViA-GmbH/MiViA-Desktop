use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use mivia_domain::ids::Timestamp;
use mivia_domain::naming::Stamp;
use mivia_ports::clock::Clock;
use mivia_ports::time::utc_stamp;

use crate::ffi;

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

    fn local_stamp(&self, at: Timestamp) -> Stamp {
        let Some([year, month, day, hour, minute]) = ffi::local_civil(at.0) else {
            return utc_stamp(at);
        };
        let small = |value: u16| u8::try_from(value).unwrap_or_default();
        Stamp {
            year,
            month: small(month),
            day: small(day),
            hour: small(hour),
            minute: small(minute),
        }
    }

    fn time_zone(&self) -> String {
        iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".to_owned())
    }
}

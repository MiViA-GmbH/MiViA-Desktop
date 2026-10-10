use async_trait::async_trait;
use mivia_domain::ids::Timestamp;
use mivia_domain::naming::Stamp;

use crate::time::utc_stamp;

#[async_trait]
pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
    async fn sleep_until(&self, deadline: Timestamp);

    fn local_stamp(&self, at: Timestamp) -> Stamp {
        utc_stamp(at)
    }

    fn time_zone(&self) -> String {
        "UTC".to_owned()
    }
}

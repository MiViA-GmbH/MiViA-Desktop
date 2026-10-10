use async_trait::async_trait;
use mivia_domain::ids::Timestamp;

#[async_trait]
pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
    async fn sleep_until(&self, deadline: Timestamp);
}

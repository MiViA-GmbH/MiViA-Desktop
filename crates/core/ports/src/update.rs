use async_trait::async_trait;
use mivia_domain::update::{Channel, Offer, Refusal};

use crate::server::ServerVersion;
use crate::telemetry::DeferReason;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateError {
    Network,
    Refused(Refusal),
    Disk,
    Install,
}

#[async_trait]
pub trait Updater: Send + Sync {
    async fn check(&self, channel: Channel) -> Result<Option<Offer>, UpdateError>;
    async fn download(&self, offer: &Offer) -> Result<u64, UpdateError>;
    async fn install(&self, version: &str) -> Result<(), UpdateError>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Activity {
    pub busy: Vec<DeferReason>,
    pub user_idle_ms: i64,
    pub accepts_notifications: bool,
}

#[async_trait]
pub trait UpdateEnv: Send + Sync {
    async fn server_version(&self) -> Option<ServerVersion>;
    async fn activity(&self) -> Activity;
    async fn hold_too_old(&self, held: bool);
}

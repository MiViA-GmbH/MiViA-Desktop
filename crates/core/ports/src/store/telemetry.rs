use super::TxError;

pub const MAX_UNSENT_EVENTS: u64 = 100_000;
pub const MAX_UNSENT_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_EVENTS_PER_DAY: u64 = 20_000;
pub const DAY_MS: i64 = 24 * 60 * 60 * 1000;
pub const SENT_KEPT_MS: i64 = 7 * DAY_MS;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewTelemetryRow {
    pub session_id: String,
    pub account_id: Option<String>,
    pub recorded_at: i64,
    pub name: String,
    pub category: String,
    pub interaction: bool,
    pub payload: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TelemetryRow {
    pub seq: i64,
    pub session_id: String,
    pub account_id: Option<String>,
    pub recorded_at: i64,
    pub name: String,
    pub category: String,
    pub interaction: bool,
    pub payload: String,
    pub batch_id: Option<String>,
    pub sent_at: Option<i64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Dropped {
    pub core: u64,
    pub interaction: u64,
}

impl Dropped {
    pub const fn total(self) -> u64 {
        self.core + self.interaction
    }

    #[must_use]
    pub const fn plus(self, other: Self) -> Self {
        Self {
            core: self.core + other.core,
            interaction: self.interaction + other.interaction,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Unsent {
    pub events: u64,
    pub bytes: u64,
    pub oldest_at: Option<i64>,
}

pub trait TelemetryRepo {
    fn append(&mut self, rows: &[NewTelemetryRow], day_limit: u64) -> Result<Dropped, TxError>;
    fn enforce_caps(&mut self, max_events: u64, max_bytes: u64) -> Result<Dropped, TxError>;
    fn save_session(&mut self, session_id: &str, resource: &str) -> Result<(), TxError>;
    fn session_resource(&mut self, session_id: &str) -> Result<Option<String>, TxError>;
    fn pending_batch(&mut self) -> Result<Vec<TelemetryRow>, TxError>;
    fn candidates(&mut self, limit: u32) -> Result<Vec<TelemetryRow>, TxError>;
    fn assign_batch(&mut self, batch_id: &str, seqs: &[i64]) -> Result<(), TxError>;
    fn release_batch(&mut self, batch_id: &str) -> Result<(), TxError>;
    fn drop_rows(&mut self, seqs: &[i64]) -> Result<(), TxError>;
    fn mark_sent(&mut self, batch_id: &str, at: i64) -> Result<(), TxError>;
    fn unsent(&mut self) -> Result<Unsent, TxError>;
    fn delete_unsent(&mut self) -> Result<u64, TxError>;
    fn prune_sent(&mut self, before: i64) -> Result<u64, TxError>;
    fn delete_sent(&mut self) -> Result<u64, TxError>;
    fn sent_since(&mut self, since: i64) -> Result<Vec<TelemetryRow>, TxError>;
}

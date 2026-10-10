use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use mivia_ports::clock::Clock;
use mivia_ports::store::telemetry::{
    Dropped, MAX_EVENTS_PER_DAY, MAX_UNSENT_BYTES, MAX_UNSENT_EVENTS, NewTelemetryRow, SENT_KEPT_MS,
};
use mivia_ports::store::{Store, StoreError, StoreExt};
use mivia_ports::telemetry::{ProductEvent, TelemetrySink};
use parking_lot::Mutex;

use super::schema::{Schema, schema};

pub const QUEUE_CAPACITY: usize = 4096;
pub const WRITE_AT: usize = 200;
const CAPS_EVERY_EVENTS: u32 = 1000;
const CAPS_EVERY_MS: i64 = 60_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SinkStats {
    pub dropped: Dropped,
    pub rejected: u64,
    pub buffered: u64,
}

#[derive(Default)]
struct Inner {
    queue: VecDeque<NewTelemetryRow>,
    account: Option<String>,
    dropped: Dropped,
    rejected: u64,
    since_caps: u32,
    caps_at: i64,
}

pub struct SqliteTelemetrySink {
    schema: &'static Schema,
    clock: Arc<dyn Clock>,
    session_id: String,
    install_id: String,
    wake: Box<dyn Fn() + Send + Sync>,
    enabled: Arc<AtomicBool>,
    inner: Mutex<Inner>,
}

impl SqliteTelemetrySink {
    pub fn new(
        clock: Arc<dyn Clock>,
        ids: (String, String),
        enabled: bool,
        wake: Box<dyn Fn() + Send + Sync>,
    ) -> Self {
        Self {
            schema: schema(),
            clock,
            session_id: ids.0,
            install_id: ids.1,
            wake,
            enabled: Arc::new(AtomicBool::new(enabled)),
            inner: Mutex::new(Inner::default()),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    pub fn set_account(&self, account: Option<String>) {
        self.inner.lock().account = account;
    }

    pub fn stats(&self) -> SinkStats {
        let inner = self.inner.lock();
        SinkStats {
            dropped: inner.dropped,
            rejected: inner.rejected,
            buffered: inner.queue.len() as u64,
        }
    }

    pub async fn begin_session(
        &self,
        store: &dyn Store,
        resource: String,
    ) -> Result<(), StoreError> {
        let id = self.session_id.clone();
        store
            .write(move |tx| tx.telemetry().save_session(&id, &resource))
            .await
    }

    pub async fn set_enabled(&self, store: &dyn Store, enabled: bool) -> Result<(), StoreError> {
        self.enabled.store(enabled, Ordering::SeqCst);
        if enabled {
            return Ok(());
        }
        self.inner.lock().queue.clear();
        store
            .write(|tx| tx.telemetry().delete_unsent())
            .await
            .map(drop)
    }

    pub async fn persist(&self, store: &dyn Store) -> Result<(), StoreError> {
        let rows: Vec<NewTelemetryRow> = self.inner.lock().queue.drain(..).collect();
        if rows.is_empty() {
            return Ok(());
        }
        let count = u32::try_from(rows.len()).unwrap_or(u32::MAX);
        let gate = Arc::clone(&self.enabled);
        let appended = store
            .write(move |tx| {
                if !gate.load(Ordering::SeqCst) {
                    return Ok(Dropped::default());
                }
                tx.telemetry().append(&rows, MAX_EVENTS_PER_DAY)
            })
            .await;
        let now = self.clock.now().0;
        let check = {
            let mut inner = self.inner.lock();
            match &appended {
                Ok(dropped) => inner.dropped = inner.dropped.plus(*dropped),
                Err(_) => {
                    inner.dropped.core += u64::from(count);
                }
            }
            inner.since_caps = inner.since_caps.saturating_add(count);
            let due = inner.since_caps >= CAPS_EVERY_EVENTS || now - inner.caps_at >= CAPS_EVERY_MS;
            if due {
                inner.since_caps = 0;
                inner.caps_at = now;
            }
            due
        };
        if check {
            self.enforce_caps(store).await?;
        }
        appended.map(drop)
    }

    pub async fn enforce_caps(&self, store: &dyn Store) -> Result<(), StoreError> {
        let dropped = store
            .write(|tx| {
                tx.telemetry()
                    .enforce_caps(MAX_UNSENT_EVENTS, MAX_UNSENT_BYTES)
            })
            .await?;
        if dropped.total() > 0 {
            tracing::warn!(
                core = dropped.core,
                interaction = dropped.interaction,
                "telemetry.capDropped"
            );
            let mut inner = self.inner.lock();
            inner.dropped = inner.dropped.plus(dropped);
        }
        Ok(())
    }

    pub async fn maintain(&self, store: &dyn Store) -> Result<(), StoreError> {
        let before = self.clock.now().0 - SENT_KEPT_MS;
        store
            .write(move |tx| tx.telemetry().prune_sent(before))
            .await?;
        self.enforce_caps(store).await
    }
}

impl TelemetrySink for SqliteTelemetrySink {
    fn record(&self, event: ProductEvent) {
        if !self.enabled() {
            return;
        }
        let Some(checked) = self.schema.encode(&event, &self.install_id) else {
            tracing::warn!(event = event.name(), "telemetry.rejected");
            self.inner.lock().rejected += 1;
            return;
        };
        let mut inner = self.inner.lock();
        if inner.queue.len() >= QUEUE_CAPACITY {
            if checked.interaction {
                inner.dropped.interaction += 1;
            } else {
                inner.dropped.core += 1;
            }
            return;
        }
        let row = NewTelemetryRow {
            session_id: self.session_id.clone(),
            account_id: inner.account.clone(),
            recorded_at: self.clock.now().0,
            name: checked.name.to_owned(),
            category: checked.category,
            interaction: checked.interaction,
            payload: checked.payload,
        };
        inner.queue.push_back(row);
        let wake = inner.queue.len() >= WRITE_AT;
        drop(inner);
        if wake {
            (self.wake)();
        }
    }
}

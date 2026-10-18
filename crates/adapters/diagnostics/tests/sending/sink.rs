use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use mivia_diagnostics::{QUEUE_CAPACITY, SqliteTelemetrySink, WRITE_AT};
use mivia_domain::ids::{FolderId, Timestamp};
use mivia_ports::store::telemetry::{
    Dropped, MAX_EVENTS_PER_DAY, MAX_UNSENT_BYTES, MAX_UNSENT_EVENTS, SENT_KEPT_MS,
};
use mivia_ports::telemetry::{FolderPaused, ProductEvent, TelemetrySink, UiView, View};
use mivia_testkit::clock::ManualClock;

use crate::NOW;
use crate::store::MemoryStore;

struct Rig {
    clock: Arc<ManualClock>,
    wakes: Arc<AtomicU32>,
    store: MemoryStore,
    sink: SqliteTelemetrySink,
}

fn rig(enabled: bool) -> Rig {
    let clock = Arc::new(ManualClock::new(Timestamp(NOW)));
    let wakes = Arc::new(AtomicU32::new(0));
    let counter = Arc::clone(&wakes);
    let sink = SqliteTelemetrySink::new(
        Arc::clone(&clock) as _,
        ("mdt-test".to_owned(), "install-a".to_owned()),
        enabled,
        Box::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        }),
    );
    Rig {
        clock,
        wakes,
        store: MemoryStore::default(),
        sink,
    }
}

fn view() -> ProductEvent {
    UiView {
        state: Some(View::SingleImages),
    }
    .into()
}

fn paused() -> ProductEvent {
    FolderPaused {
        folder_id: Some(FolderId(7)),
    }
    .into()
}

impl Rig {
    fn record(&self, events: usize) {
        for _ in 0..events {
            self.sink.record(view());
        }
    }

    fn wakes(&self) -> u32 {
        self.wakes.load(Ordering::SeqCst)
    }

    fn caps(&self) -> usize {
        self.store.tables.lock().caps.len()
    }
}

#[tokio::test]
async fn events_wait_in_memory_and_wake_the_writer_when_first_or_many() {
    let rig = rig(true);
    rig.sink.set_account(Some("acct-1".to_owned()));
    rig.record(1);
    assert_eq!((rig.sink.stats().buffered, rig.wakes()), (1, 1));
    rig.record(WRITE_AT - 2);
    assert_eq!(rig.wakes(), 1);
    rig.record(1);
    assert_eq!(rig.wakes(), 2);
    rig.sink.persist(&rig.store).await.expect("persisted");
    assert_eq!(rig.sink.stats().buffered, 0);
    let tables = rig.store.tables.lock();
    assert_eq!(tables.rows.len(), WRITE_AT);
    assert_eq!(tables.day_limit, Some(MAX_EVENTS_PER_DAY));
    let row = &tables.rows[0];
    assert_eq!(row.account_id.as_deref(), Some("acct-1"));
    assert_eq!(
        (row.session_id.as_str(), row.recorded_at),
        ("mdt-test", NOW)
    );
    assert_eq!((row.name.as_str(), row.interaction), ("ui.view", true));
}

#[tokio::test]
async fn a_disabled_sink_keeps_nothing_and_clears_what_was_waiting() {
    let rig = rig(false);
    assert!(!rig.sink.enabled());
    rig.record(1);
    assert_eq!(rig.sink.stats().buffered, 0);
    let rig = self::rig(true);
    rig.store.tables.lock().add(NOW, "{}");
    rig.record(1);
    rig.sink.set_enabled(&rig.store, true).await.expect("set");
    assert_eq!(rig.store.tables.lock().unsent_deleted, 0);
    rig.sink.set_enabled(&rig.store, false).await.expect("set");
    assert!(!rig.sink.enabled());
    assert_eq!(rig.sink.stats().buffered, 0);
    let tables = rig.store.tables.lock();
    assert_eq!(tables.unsent_deleted, 1);
    assert_eq!(tables.rows.len(), 0);
}

#[tokio::test]
async fn a_full_queue_counts_drops_by_priority() {
    let rig = rig(true);
    rig.record(QUEUE_CAPACITY);
    rig.record(1);
    rig.sink.record(paused());
    let stats = rig.sink.stats();
    assert_eq!(stats.buffered, QUEUE_CAPACITY as u64);
    assert_eq!(
        stats.dropped,
        Dropped {
            core: 1,
            interaction: 1
        }
    );
}

#[tokio::test]
async fn failed_writes_count_as_dropped() {
    let rig = rig(true);
    rig.record(3);
    *rig.store.broken.lock() = true;
    assert!(rig.sink.persist(&rig.store).await.is_err());
    assert_eq!(rig.sink.stats().dropped.core, 3);
}

#[tokio::test]
async fn caps_are_enforced_every_minute_or_thousand_events() {
    let rig = rig(true);
    rig.store.tables.lock().cap_drop = Dropped {
        core: 2,
        interaction: 1,
    };
    rig.record(1);
    rig.sink.persist(&rig.store).await.expect("persisted");
    assert_eq!(rig.caps(), 1);
    assert_eq!(rig.sink.stats().dropped.total(), 3);
    rig.clock.advance(59_999);
    rig.record(1);
    rig.sink.persist(&rig.store).await.expect("persisted");
    assert_eq!(rig.caps(), 1);
    rig.clock.advance(1);
    rig.record(1);
    rig.sink.persist(&rig.store).await.expect("persisted");
    assert_eq!(rig.caps(), 2);
    rig.record(999);
    rig.sink.persist(&rig.store).await.expect("persisted");
    assert_eq!(rig.caps(), 2);
    rig.record(1);
    rig.sink.persist(&rig.store).await.expect("persisted");
    assert_eq!(rig.caps(), 3);
}

#[tokio::test]
async fn maintenance_prunes_old_sent_rows_and_enforces_caps() {
    let rig = rig(true);
    rig.sink
        .begin_session(&rig.store, r#"{"os":"windows"}"#.to_owned())
        .await
        .expect("saved");
    rig.sink.maintain(&rig.store).await.expect("maintained");
    let tables = rig.store.tables.lock();
    assert_eq!(
        tables.sessions.get("mdt-test").map(String::as_str),
        Some(r#"{"os":"windows"}"#)
    );
    assert_eq!(tables.pruned_before, [NOW - SENT_KEPT_MS]);
    assert_eq!(tables.caps, [(MAX_UNSENT_EVENTS, MAX_UNSENT_BYTES)]);
}

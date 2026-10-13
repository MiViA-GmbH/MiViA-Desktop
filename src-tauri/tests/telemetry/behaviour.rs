use mivia_diagnostics::{Flush, Step, sent_batches};
use mivia_ports::store::StoreExt;
use mivia_ports::store::telemetry::SENT_KEPT_MS;

use crate::{KEY, NOW, decode, events, world};

#[tokio::test(flavor = "multi_thread")]
async fn batches_hold_core_events_before_interaction_and_stay_inside_the_limits() {
    let world = world(None).await;
    world.record(0, 300).await;
    world.record(900, 0).await;
    let mut now = NOW;
    let mut sent = 0;
    while world.unsent().await > 0 {
        let Step::Sent { events } = world.now(Flush::Now, now).await else {
            panic!("a batch should be sent");
        };
        sent += events;
        now += 31_000;
    }
    assert_eq!(sent, 1200);
    for (index, (_, body)) in world.stub.engine.telemetry_batches().iter().enumerate() {
        let batch = decode(body);
        let list = events(&batch);
        assert!(list.len() <= 500, "batch {index}");
        assert!(
            serde_json::to_vec(&batch).unwrap().len() <= 96_000,
            "batch {index}"
        );
        let categories: Vec<bool> = list
            .iter()
            .map(|event| event["name"] == "ui.view")
            .collect();
        assert!(
            categories.windows(2).all(|pair| pair[0] <= pair[1]),
            "core first {index}"
        );
        for kind in [false, true] {
            let seqs: Vec<i64> = list
                .iter()
                .filter(|event| (event["name"] == "ui.view") == kind)
                .map(|event| event["seq"].as_i64().unwrap())
                .collect();
            assert!(
                seqs.windows(2).all(|pair| pair[0] < pair[1]),
                "ascending {index}"
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn pace_and_flush_triggers() {
    let world = world(None).await;
    let five_minutes = 5 * 60_000;
    world.record(2, 0).await;
    assert_eq!(
        world.now(Flush::Regular, NOW).await,
        Step::Sent { events: 2 }
    );
    assert_eq!(world.now(Flush::Regular, NOW + 1_000).await, Step::Idle);
    assert_eq!(world.now(Flush::Regular, NOW + 6_000).await, Step::Idle);
    world.record(1, 0).await;
    assert_eq!(world.now(Flush::Regular, NOW + 12_000).await, Step::Idle);
    let aged = world.now(Flush::Regular, NOW + five_minutes).await;
    assert_eq!(aged, Step::Sent { events: 1 });
    world.record(250, 0).await;
    let busy = NOW + five_minutes;
    assert_eq!(
        world.step(Flush::Regular, busy + 2_000, (true, true)).await,
        Step::Idle
    );
    let full = world.step(Flush::Regular, busy + 6_000, (true, true)).await;
    assert_eq!(full, Step::Sent { events: 250 });
    world.record(260, 0).await;
    assert_eq!(
        world
            .step(Flush::Regular, busy + 20_000, (true, true))
            .await,
        Step::Idle
    );
    let later = world
        .step(Flush::Regular, busy + 37_000, (true, true))
        .await;
    assert_eq!(later, Step::Sent { events: 260 });
}

#[tokio::test(flavor = "multi_thread")]
async fn the_start_flush_waits_for_the_first_recorded_events() {
    let world = world(None).await;
    assert_eq!(world.now(Flush::Regular, NOW).await, Step::Idle);
    world.record(2, 0).await;
    assert_eq!(
        world.now(Flush::Regular, NOW + 6_000).await,
        Step::Sent { events: 2 }
    );
    world.record(1, 0).await;
    assert_eq!(world.now(Flush::Regular, NOW + 12_000).await, Step::Idle);
}

#[tokio::test(flavor = "multi_thread")]
async fn one_request_is_in_flight_at_a_time() {
    let world = world(None).await;
    world.record(3, 0).await;
    let (first, second) = tokio::join!(world.now(Flush::Now, NOW), world.now(Flush::Now, NOW));
    let sent = [first, second]
        .into_iter()
        .filter(|step| matches!(step, Step::Sent { .. }))
        .count();
    assert_eq!(sent, 1);
    assert_eq!(world.requests(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn nothing_is_sent_until_the_gate_is_open() {
    let world = world(None).await;
    world.record(2, 0).await;
    assert_eq!(
        world.step(Flush::Now, NOW, (false, false)).await,
        Step::Stopped
    );
    assert_eq!(world.requests(), 0);
    assert_eq!(world.unsent().await, 2);
    assert_eq!(world.now(Flush::Now, NOW).await, Step::Sent { events: 2 });
}

#[tokio::test(flavor = "multi_thread")]
async fn opting_out_deletes_unsent_rows_in_one_step_and_records_nothing_more() {
    let world = world(None).await;
    world.record(4, 2).await;
    assert_eq!(world.unsent().await, 6);
    world
        .sink
        .set_enabled(world.store.as_ref(), false)
        .await
        .unwrap();
    assert_eq!(world.unsent().await, 0);
    world.record(3, 1).await;
    assert_eq!(world.unsent().await, 0);
    assert!(!world.sink.enabled());
    assert_eq!(world.now(Flush::Now, NOW).await, Step::Idle);
    assert_eq!(world.requests(), 0);
    world
        .sink
        .set_enabled(world.store.as_ref(), true)
        .await
        .unwrap();
    world.record(1, 0).await;
    assert_eq!(world.unsent().await, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_daily_cap_counts_what_it_drops() {
    let world = world(None).await;
    for _ in 0..5 {
        world.record(4_000, 0).await;
    }
    world.record(100, 100).await;
    assert_eq!(world.unsent().await, 20_000);
    let stats = world.sink.stats();
    assert_eq!(stats.dropped.core + stats.dropped.interaction, 100 + 100);
}

#[tokio::test(flavor = "multi_thread")]
async fn sent_rows_stay_for_seven_days_then_go() {
    let world = world(None).await;
    world.record(2, 1).await;
    assert_eq!(world.now(Flush::Now, NOW).await, Step::Sent { events: 3 });
    let rows = world
        .store
        .write(|tx| tx.telemetry().sent_since(0))
        .await
        .unwrap();
    let view = sent_batches(&rows, Some("1"));
    assert_eq!((view.len(), view[0].events), (1, 3));
    world.clock.advance(SENT_KEPT_MS - 1000);
    world.sink.maintain(world.store.as_ref()).await.unwrap();
    let kept = world
        .store
        .write(|tx| tx.telemetry().sent_since(0))
        .await
        .unwrap();
    assert_eq!(kept.len(), 3);
    world.clock.advance(2000);
    world.sink.maintain(world.store.as_ref()).await.unwrap();
    let gone = world
        .store
        .write(|tx| tx.telemetry().sent_since(0))
        .await
        .unwrap();
    assert_eq!(gone.len(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn canary_values_never_reach_telemetry_rows_bodies_or_views() {
    let world = world(None).await;
    let canaries = [KEY, "canary.owner@lab.example", "canaryuser", "Probe 7"];
    world.record(2, 1).await;
    world.now(Flush::Now, NOW).await;
    let rows = world
        .store
        .write(|tx| tx.telemetry().sent_since(0))
        .await
        .unwrap();
    let mut seen = vec![format!("{rows:?}")];
    seen.extend(
        sent_batches(&rows, Some("1"))
            .into_iter()
            .map(|batch| batch.json),
    );
    seen.extend(
        world
            .stub
            .engine
            .telemetry_batches()
            .iter()
            .map(|(_, body)| decode(body).to_string()),
    );
    for text in seen {
        let lower = text.to_lowercase();
        for canary in canaries {
            assert!(
                !lower.contains(&canary.to_lowercase()),
                "{canary} leaked in {text}"
            );
        }
    }
}

#[test]
fn the_writer_is_woken_by_the_first_queued_event_and_at_the_batch_size() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use mivia_diagnostics::{SqliteTelemetrySink, WRITE_AT};
    use mivia_domain::ids::{FolderId, Timestamp};
    use mivia_ports::telemetry::{FolderPaused, TelemetrySink};
    use mivia_testkit::clock::ManualClock;

    let wakes = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&wakes);
    let sink = SqliteTelemetrySink::new(
        Arc::new(ManualClock::new(Timestamp(NOW))),
        ("mdt-test".to_owned(), "install".to_owned()),
        true,
        Box::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        }),
    );
    let record = |id: i64| {
        sink.record(
            FolderPaused {
                folder_id: Some(FolderId(id)),
            }
            .into(),
        );
    };
    record(0);
    record(1);
    assert_eq!(wakes.load(Ordering::SeqCst), 1);
    for id in 2..i64::try_from(WRITE_AT).unwrap() {
        record(id);
    }
    assert_eq!(wakes.load(Ordering::SeqCst), 2);
}

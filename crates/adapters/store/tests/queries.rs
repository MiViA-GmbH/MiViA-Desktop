use mivia_domain::computation::{ComputationEvent, JobOutcome};
use mivia_domain::hold::{Access, Gate, HoldReason};
use mivia_domain::ids::{BatchId, Timestamp};
use mivia_domain::label::LabelRule;
use mivia_domain::path::WinPath;
use mivia_domain::source_file::SourceFileEvent;
use mivia_ports::queries::Queries;
use mivia_ports::store::{NewComputation, NewFolder, NewReport, SourceClaim, StoreExt};
use mivia_store::StoreConfig;

use crate::common::{account, closed_batch, fixture, new_batch, new_file, seed};

#[tokio::test]
async fn history_pages_by_closing_order_with_open_head() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let scope = seed(&fx.store, |tx| {
        let scope = account(tx, "a");
        for n in 0..102 {
            closed_batch(tx, scope, &format!("S{n}"));
        }
        tx.batches(scope).open(&new_batch("open")).unwrap();
        scope
    })
    .await;
    let first = fx.store.list_history(scope, None).await.unwrap();
    assert_eq!(first.head.len(), 1);
    assert_eq!(first.rows.len(), 100);
    assert_eq!(first.rows[0].label, "S101");
    let next = first.next.clone().unwrap();
    assert_eq!(next.id, first.rows[99].id);
    let second = fx.store.list_history(scope, Some(next)).await.unwrap();
    assert_eq!(second.head, []);
    let labels: Vec<_> = second.rows.iter().map(|row| row.label.as_str()).collect();
    assert_eq!(labels, ["S1", "S0"]);
    assert_eq!(second.next, None);
}

#[tokio::test]
async fn problems_raise_attention_and_items_list_in_arrival_order() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let (scope, batch) = seed(&fx.store, |tx| {
        let scope = account(tx, "a");
        let batch = tx.batches(scope).open(&new_batch("S")).unwrap();
        let first = tx
            .source_files(scope)
            .add(&new_file(batch, "b.tif"))
            .unwrap();
        tx.source_files(scope)
            .add(&new_file(batch, "a.tif"))
            .unwrap();
        for event in [SourceFileEvent::ReadinessPassed, SourceFileEvent::TooLarge] {
            tx.source_files(scope)
                .apply(first, event, Timestamp(3))
                .unwrap();
        }
        (scope, batch)
    })
    .await;
    let attention = fx.store.list_needs_attention(scope).await.unwrap();
    assert_eq!(attention.series_total, 1);
    assert_eq!(attention.series[0].counts.problems, 1);
    let view = fx.store.get_batch(scope, batch).await.unwrap().unwrap();
    assert_eq!(view.summary.counts.files, 2);
    assert!(view.summary.needs_attention);
    let items = fx.store.list_batch_items(scope, batch, None).await.unwrap();
    let states: Vec<_> = items
        .items
        .iter()
        .map(|i| (i.label.as_str(), i.state.as_str()))
        .collect();
    assert_eq!(states, [("b.tif", "rejected"), ("a.tif", "detected")]);
    assert_eq!(fx.store.get_batch(scope, BatchId(99)).await.unwrap(), None);
}

#[tokio::test]
async fn folders_list_by_position_and_claims_follow_holds() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let scope = seed(&fx.store, |tx| {
        let scope = account(tx, "a");
        for (position, name) in [(1, "B"), (0, "A")] {
            tx.folders(scope)
                .add(&NewFolder {
                    path: WinPath::parse(&format!(r"D:\{name}")).unwrap(),
                    position,
                    quiet_period_ms: 2_000,
                    label_rule: LabelRule::Auto,
                    report_settings_json: "{}".to_owned(),
                })
                .unwrap();
        }
        let batch = tx.batches(scope).open(&new_batch("S")).unwrap();
        let file = tx
            .source_files(scope)
            .add(&new_file(batch, "a.tif"))
            .unwrap();
        tx.source_files(scope)
            .apply(file, SourceFileEvent::ReadinessPassed, Timestamp(1_000))
            .unwrap();
        scope
    })
    .await;
    let folders = fx.store.list_folders(scope).await.unwrap();
    let paths: Vec<_> = folders.folders.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, [r"D:\A", r"D:\B"]);
    let due = move |tx: &mut dyn mivia_ports::store::WriteTx| {
        tx.source_files(scope)
            .due(SourceClaim::Upload, Timestamp(5_000), &[])
    };
    assert_eq!(fx.store.write(due).await.unwrap().len(), 1);
    let blocked = fx
        .store
        .write(move |tx| {
            tx.holds(scope)
                .raise(HoldReason::Offline, Timestamp(1), Some(Timestamp(2)))?;
            tx.holds(scope).access(Gate::Upload)
        })
        .await
        .unwrap();
    assert_eq!(blocked, Access::Blocked);
    assert_eq!(fx.store.write(due).await.unwrap(), []);
    let probes = fx
        .store
        .write(move |tx| tx.holds(scope).due_for_probe(Timestamp(5)))
        .await
        .unwrap();
    assert_eq!(probes, [HoldReason::Offline]);
}

#[tokio::test]
async fn claims_follow_state_and_skip_in_flight_ids() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let claims = seed(&fx.store, |tx| {
        let scope = account(tx, "a");
        let batch = tx.batches(scope).open(&new_batch("S")).unwrap();
        let file = tx
            .source_files(scope)
            .add(&new_file(batch, "a.tif"))
            .unwrap();
        let computation = tx
            .computations(scope)
            .add(&NewComputation {
                batch_id: batch,
                source_file_id: file,
                model_choice_key: "grain:".to_owned(),
                model_version: None,
                app_version: "2.0.0".to_owned(),
                next_attempt_at: Timestamp(1_000),
            })
            .unwrap();
        let now = Timestamp(2_000);
        let submit = tx.computations(scope).due_for_submit(now, &[]).unwrap();
        let in_flight = tx
            .computations(scope)
            .due_for_submit(now, &[computation])
            .unwrap();
        for event in [
            ComputationEvent::SubmitClaimed,
            ComputationEvent::Job(JobOutcome::Pending),
        ] {
            tx.computations(scope)
                .apply(computation, event, now)
                .unwrap();
        }
        let poll = tx.computations(scope).due_for_poll(now, &[]).unwrap();
        let report = tx
            .reports(scope)
            .create(&NewReport {
                batch_id: batch,
                model_choice_key: "grain:".to_owned(),
                formats: vec!["pdf".to_owned()],
                language: "en".to_owned(),
                created_at: now,
                app_version: "2.0.0".to_owned(),
                members: vec![(computation, "a.tif".to_owned())],
            })
            .unwrap();
        let parts = tx.reports(scope).parts(report).unwrap();
        let request = tx.reports(scope).due_for_request(&[]).unwrap();
        let excluded = tx.reports(scope).due_for_request(&parts).unwrap();
        let report_poll = tx.reports(scope).due_for_poll(now, &[]).unwrap();
        (
            submit == [computation],
            in_flight.is_empty(),
            poll == [computation],
            request == parts,
            excluded.is_empty(),
            report_poll.is_empty(),
        )
    })
    .await;
    assert_eq!(claims, (true, true, true, true, true, true));
}

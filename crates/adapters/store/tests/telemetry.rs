use mivia_ports::store::StoreExt;
use mivia_ports::store::telemetry::{DAY_MS, NewTelemetryRow};
use mivia_store::StoreConfig;

use crate::common::fixture;

fn row(session: &str, at: i64, interaction: bool) -> NewTelemetryRow {
    NewTelemetryRow {
        session_id: session.to_owned(),
        account_id: None,
        recorded_at: at,
        name: "ui.view".to_owned(),
        category: "usage".to_owned(),
        interaction,
        payload: "{}".to_owned(),
    }
}

#[tokio::test]
async fn sequence_numbers_survive_deleting_everything() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let seqs = fx
        .store
        .write(|tx| {
            tx.telemetry()
                .append(&[row("a", 1, false), row("a", 2, false)], 100)?;
            tx.telemetry().delete_unsent()?;
            tx.telemetry().append(&[row("a", 3, false)], 100)?;
            tx.telemetry().candidates(10)
        })
        .await
        .unwrap();
    assert_eq!(seqs.iter().map(|r| r.seq).collect::<Vec<_>>(), vec![3]);
}

#[tokio::test]
async fn the_daily_cap_drops_interaction_first_and_counts_core() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let (dropped, kept, next_day) = fx
        .store
        .write(|tx| {
            let rows = [
                row("a", 1, false),
                row("a", 2, true),
                row("a", 3, true),
                row("a", 4, false),
            ];
            let dropped = tx.telemetry().append(&rows, 2)?;
            let kept = tx.telemetry().unsent()?.events;
            tx.telemetry().append(&[row("a", DAY_MS + 1, false)], 2)?;
            Ok((dropped, kept, tx.telemetry().unsent()?.events))
        })
        .await
        .unwrap();
    assert_eq!((dropped.core, dropped.interaction), (1, 1));
    assert_eq!((kept, next_day), (2, 3));
}

#[tokio::test]
async fn caps_delete_the_oldest_interaction_events_before_core() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let (dropped, left) = fx
        .store
        .write(|tx| {
            let rows = [
                row("a", 1, true),
                row("a", 2, false),
                row("a", 3, true),
                row("a", 4, false),
                row("a", 5, false),
            ];
            tx.telemetry().append(&rows, 100)?;
            let dropped = tx.telemetry().enforce_caps(2, u64::MAX)?;
            let left = tx.telemetry().candidates(10)?;
            Ok((dropped, left))
        })
        .await
        .unwrap();
    assert_eq!((dropped.core, dropped.interaction), (1, 2));
    assert_eq!(left.iter().map(|r| r.seq).collect::<Vec<_>>(), vec![4, 5]);
}

#[tokio::test]
async fn bytes_over_the_cap_delete_oldest_first() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let left = fx
        .store
        .write(|tx| {
            let rows: Vec<_> = (1..=4).map(|n| row("a", n, false)).collect();
            tx.telemetry().append(&rows, 100)?;
            tx.telemetry().enforce_caps(100, 4)?;
            Ok(tx.telemetry().unsent()?.events)
        })
        .await
        .unwrap();
    assert_eq!(left, 2);
}

#[tokio::test]
async fn a_batch_is_one_session_ordered_core_first_and_resent_whole() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let (first, again, after_sent) = fx
        .store
        .write(|tx| {
            let rows = [
                row("a", 1, true),
                row("b", 2, false),
                row("a", 3, false),
                row("a", 4, true),
            ];
            tx.telemetry().append(&rows, 100)?;
            let first = tx.telemetry().candidates(10)?;
            let seqs: Vec<i64> = first.iter().map(|r| r.seq).collect();
            tx.telemetry().assign_batch("batch-1", &seqs)?;
            let again = tx.telemetry().pending_batch()?;
            tx.telemetry().mark_sent("batch-1", 10)?;
            let after_sent = tx.telemetry().candidates(10)?;
            Ok((first, again, after_sent))
        })
        .await
        .unwrap();
    let order = |rows: &[mivia_ports::store::telemetry::TelemetryRow]| {
        rows.iter().map(|r| r.seq).collect::<Vec<_>>()
    };
    assert_eq!(order(&first), vec![3, 1, 4]);
    assert_eq!(order(&again), vec![3, 1, 4]);
    assert_eq!(order(&after_sent), vec![2]);
}

#[tokio::test]
async fn opt_out_pruning_and_sent_history() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let (deleted, pruned, sent) = fx
        .store
        .write(|tx| {
            tx.telemetry()
                .append(&[row("a", 1, false), row("a", 2, false)], 100)?;
            let seqs: Vec<i64> = tx
                .telemetry()
                .candidates(1)?
                .iter()
                .map(|r| r.seq)
                .collect();
            tx.telemetry().assign_batch("b", &seqs)?;
            tx.telemetry().mark_sent("b", 5)?;
            let deleted = tx.telemetry().delete_unsent()?;
            let sent = tx.telemetry().sent_since(0)?.len();
            let pruned = tx.telemetry().prune_sent(6)?;
            Ok((deleted, pruned, sent))
        })
        .await
        .unwrap();
    assert_eq!((deleted, pruned, sent), (1, 1, 1));
}

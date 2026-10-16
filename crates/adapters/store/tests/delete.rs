use mivia_domain::computation::{ComputationEvent, JobOutcome};
use mivia_domain::ids::{BatchId, SourceFileId, Timestamp};
use mivia_domain::source_file::SourceFileEvent;
use mivia_domain::transition::IllegalTransition;
use mivia_ports::ipc::history::HistoryFilter;
use mivia_ports::queries::Queries;
use mivia_ports::store::history::{DeleteRequest, DeleteTarget, Deletion};
use mivia_ports::store::pipeline::JobRecord;
use mivia_ports::store::{StoreError, StoreExt, TxError};
use mivia_store::{SqliteStore, StoreConfig};

use crate::common::{account, fixture, new_file, seed};
use crate::history::{SCOPE, folder, series};
use crate::history_reports::report_with_saved_part;
use crate::reports::{DONE, NOW, add, finish};

const LATER: Timestamp = Timestamp(20_000);

async fn delete(
    store: &SqliteStore,
    target: DeleteTarget,
    report_files: bool,
) -> Result<Deletion, StoreError> {
    let request = DeleteRequest {
        target,
        report_files,
        server: true,
    };
    store
        .write(move |tx| tx.history(SCOPE).delete(request, LATER))
        .await
}

async fn actions(store: &SqliteStore, batch: BatchId) -> Vec<(String, Option<String>)> {
    let page = store.list_batch_events(SCOPE, batch, None).await.unwrap();
    page.events
        .into_iter()
        .map(|event| (event.action, event.after))
        .collect()
}

#[tokio::test]
async fn deleting_an_image_cancels_its_work_and_recounts_the_series() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let batch = seed(&fx.store, |tx| {
        account(tx, "a");
        let origin = folder(tx, SCOPE, r"D:\Lab");
        let batch = series(tx, origin, "Run", "grain");
        let done = add(tx, SCOPE, batch, "a.tif");
        finish(tx, SCOPE, done, DONE);
        add(tx, SCOPE, batch, "b.tif");
        batch
    })
    .await;
    let queued = delete(
        &fx.store,
        DeleteTarget::Image(batch, SourceFileId(2)),
        false,
    )
    .await
    .unwrap();
    assert_eq!((queued.images, queued.jobs.len()), (1, 0));
    let done = delete(
        &fx.store,
        DeleteTarget::Image(batch, SourceFileId(1)),
        false,
    )
    .await
    .unwrap();
    assert_eq!(done.jobs, ["job-1"]);
    let summary = fx.store.get_batch(SCOPE, batch).await.unwrap().unwrap();
    let counts = summary.summary.counts;
    assert_eq!((counts.files, counts.done, counts.active), (0, 0, 0));
    let items = fx.store.list_batch_items(SCOPE, batch, None).await.unwrap();
    assert_eq!(items.items, []);
    let pending = fx
        .store
        .write(|tx| tx.computations(SCOPE).due_for_submit(LATER, &[]))
        .await
        .unwrap();
    assert_eq!(pending, []);
    let logged = actions(&fx.store, batch).await;
    let deleted = logged
        .iter()
        .filter(|(action, _)| action == "deleted")
        .count();
    assert_eq!(deleted, 2);
    assert!(
        logged[0]
            .1
            .as_deref()
            .unwrap()
            .contains("\"label\":\"a.tif\"")
    );
    let again = delete(
        &fx.store,
        DeleteTarget::Image(batch, SourceFileId(1)),
        false,
    )
    .await;
    assert_eq!(again, Err(StoreError::Tx(TxError::NotFound)));
}

#[tokio::test]
async fn deleting_a_series_hides_it_and_its_reports_but_keeps_the_changes() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let (batch, _) = seed(&fx.store, |tx| {
        account(tx, "a");
        report_with_saved_part(tx, "Run")
    })
    .await;
    let deleted = delete(&fx.store, DeleteTarget::Series(batch), true)
        .await
        .unwrap();
    assert_eq!(deleted.jobs, ["job-1"]);
    assert_eq!(deleted.parts.len(), 1);
    assert!(
        deleted.parts[0]
            .path
            .to_string()
            .ends_with("Grain size.pdf")
    );
    assert!(fx.store.get_batch(SCOPE, batch).await.unwrap().is_none());
    let page = fx
        .store
        .list_history(SCOPE, HistoryFilter::default(), None)
        .await
        .unwrap();
    assert_eq!((page.head, page.rows), (Vec::new(), Vec::new()));
    let searched = HistoryFilter {
        q: Some("Run".to_owned()),
        ..HistoryFilter::default()
    };
    let found = fx.store.list_history(SCOPE, searched, None).await.unwrap();
    assert_eq!(found.rows, []);
    let logged = actions(&fx.store, batch).await;
    assert_eq!(logged[0].0, "deleted");
    assert!(logged.iter().any(|(action, _)| action == "report_saved"));
    let again = delete(&fx.store, DeleteTarget::Series(batch), false).await;
    assert_eq!(again, Err(StoreError::Tx(TxError::NotFound)));
}

#[tokio::test]
async fn deleting_an_open_series_closes_it_without_building_reports() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let batch = seed(&fx.store, |tx| {
        account(tx, "a");
        let origin = folder(tx, SCOPE, r"D:\Lab");
        let batch = series(tx, origin, "Run", "grain");
        let done = add(tx, SCOPE, batch, "a.tif");
        finish(tx, SCOPE, done, DONE);
        add(tx, SCOPE, batch, "b.tif");
        batch
    })
    .await;
    delete(&fx.store, DeleteTarget::Series(batch), false)
        .await
        .unwrap();
    let waiting = fx
        .store
        .write(|tx| tx.reports(SCOPE).waiting())
        .await
        .unwrap();
    assert_eq!(waiting.len(), 0);
    let open = fx
        .store
        .write(|tx| tx.batches(SCOPE).open_batches())
        .await
        .unwrap();
    assert_eq!(open, []);
}

#[tokio::test]
async fn work_that_finishes_after_the_deletion_is_dropped() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let (batch, computing, uploading) = seed(&fx.store, |tx| {
        account(tx, "a");
        let origin = folder(tx, SCOPE, r"D:\Lab");
        let batch = series(tx, origin, "Run", "grain");
        let computing = add(tx, SCOPE, batch, "a.tif");
        finish(tx, SCOPE, computing, JobOutcome::Pending);
        let uploading = tx
            .source_files(SCOPE)
            .add(&new_file(batch, "b.tif"))
            .unwrap();
        for event in [
            SourceFileEvent::ReadinessPassed,
            SourceFileEvent::UploadClaimed,
        ] {
            tx.source_files(SCOPE).apply(uploading, event, NOW).unwrap();
        }
        (batch, computing, uploading)
    })
    .await;
    delete(&fx.store, DeleteTarget::Series(batch), false)
        .await
        .unwrap();
    let job = JobRecord {
        job_id: "job-1".to_owned(),
        outcome: DONE,
        result_id: None,
        submitted_at: NOW,
        next_poll_at: NOW,
    };
    let state = fx
        .store
        .write(move |tx| {
            tx.pipeline(SCOPE).uploaded(uploading, "image-2", LATER)?;
            tx.pipeline(SCOPE).record_job(computing, &job, LATER)?;
            tx.pipeline(SCOPE)
                .computation_event(computing, ComputationEvent::Ambiguous, LATER)?;
            tx.computations(SCOPE)
                .apply(computing, ComputationEvent::Cancel, LATER)
        })
        .await;
    assert_eq!(
        state,
        Err(StoreError::Tx(TxError::Illegal(IllegalTransition(
            "computation"
        ))))
    );
}

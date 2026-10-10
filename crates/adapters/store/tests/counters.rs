use mivia_domain::batch::ClosureReason;
use mivia_domain::computation::{ComputationEvent, FailReason, JobOutcome};
use mivia_domain::ids::{BatchId, ComputationId, ReportId, ReportPartId, SourceFileId, Timestamp};
use mivia_domain::report::{PartEvent, SaveError};
use mivia_domain::source_file::{Classification, OpenFailure, SourceFileEvent, UnreadableReason};
use mivia_ports::store::{AccountScope, NewComputation, NewReport, StoreExt, TxError};
use mivia_store::{SqliteStore, StoreConfig};
use proptest::prelude::*;
use proptest::test_runner::Config;
use rusqlite::Connection;

use crate::common::{account, fixture, new_batch, new_file, seed};

const FILE_EVENTS: [SourceFileEvent; 12] = [
    SourceFileEvent::Removed,
    SourceFileEvent::ContentChanged,
    SourceFileEvent::ReadinessPassed,
    SourceFileEvent::Gone,
    SourceFileEvent::GaveUp(UnreadableReason::InUse),
    SourceFileEvent::Classified(Classification::New),
    SourceFileEvent::Classified(Classification::KnownFile),
    SourceFileEvent::TooLarge,
    SourceFileEvent::UploadClaimed,
    SourceFileEvent::UploadSucceeded,
    SourceFileEvent::UploadRejected,
    SourceFileEvent::UploadOpenFailed(OpenFailure::Gone),
];

const COMPUTATION_EVENTS: [ComputationEvent; 9] = [
    ComputationEvent::SubmitClaimed,
    ComputationEvent::Job(JobOutcome::Pending),
    ComputationEvent::Job(JobOutcome::Done { reused: false }),
    ComputationEvent::Job(JobOutcome::Failed(FailReason::ServerFailed)),
    ComputationEvent::Ambiguous,
    ComputationEvent::ChoiceUnavailable,
    ComputationEvent::JobGone { resubmitted: false },
    ComputationEvent::Cancel,
    ComputationEvent::RunAgain,
];

const PART_EVENTS: [PartEvent; 6] = [
    PartEvent::Saved,
    PartEvent::SaveFailed(SaveError::Retryable),
    PartEvent::PendingTooLong,
    PartEvent::TryAgain,
    PartEvent::Discard,
    PartEvent::Cancelled { rerequests: 3 },
];

#[derive(Clone, Debug)]
enum Op {
    AddFile,
    File(usize, usize),
    AddComputation(usize),
    Computation(usize, usize),
    Report(usize),
    Part(usize, usize),
    Stale(usize, bool),
    Close,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        Just(Op::AddFile),
        (any::<usize>(), 0..FILE_EVENTS.len()).prop_map(|(i, e)| Op::File(i, e)),
        any::<usize>().prop_map(Op::AddComputation),
        (any::<usize>(), 0..COMPUTATION_EVENTS.len()).prop_map(|(i, e)| Op::Computation(i, e)),
        (1_usize..4).prop_map(Op::Report),
        (any::<usize>(), 0..PART_EVENTS.len()).prop_map(|(i, e)| Op::Part(i, e)),
        (any::<usize>(), any::<bool>()).prop_map(|(i, s)| Op::Stale(i, s)),
        Just(Op::Close),
    ]
}

#[derive(Default)]
struct Ids {
    files: Vec<SourceFileId>,
    computations: Vec<ComputationId>,
    reports: Vec<ReportId>,
    parts: Vec<ReportPartId>,
}

fn pick<T: Copy>(items: &[T], index: usize) -> Option<T> {
    (!items.is_empty()).then(|| items[index % items.len()])
}

async fn run(store: &SqliteStore, scope: AccountScope, batch: BatchId, op: Op, ids: &mut Ids) {
    let now = Timestamp(2_000);
    let result = match op {
        Op::AddFile => store
            .write(move |tx| tx.source_files(scope).add(&new_file(batch, "a.tif")))
            .await
            .map(|id| ids.files.push(id)),
        Op::File(i, e) => match pick(&ids.files, i) {
            Some(id) => store
                .write(move |tx| tx.source_files(scope).apply(id, FILE_EVENTS[e], now))
                .await
                .map(drop),
            None => Ok(()),
        },
        Op::AddComputation(i) => match pick(&ids.files, i) {
            Some(file) => store
                .write(move |tx| {
                    tx.computations(scope).add(&NewComputation {
                        batch_id: batch,
                        source_file_id: file,
                        model_choice_key: "grain:".to_owned(),
                        model_version: None,
                        app_version: "2.0.0".to_owned(),
                        next_attempt_at: now,
                    })
                })
                .await
                .map(|id| ids.computations.push(id)),
            None => Ok(()),
        },
        Op::Computation(i, e) => match pick(&ids.computations, i) {
            Some(id) => store
                .write(move |tx| tx.computations(scope).apply(id, COMPUTATION_EVENTS[e], now))
                .await
                .map(drop),
            None => Ok(()),
        },
        Op::Report(formats) => {
            let members = ids
                .computations
                .iter()
                .map(|id| (*id, "m".to_owned()))
                .collect();
            store
                .write(move |tx| {
                    let report = tx.reports(scope).create(&NewReport {
                        batch_id: batch,
                        model_choice_key: "grain:".to_owned(),
                        formats: ["pdf", "csv", "zip"]
                            .iter()
                            .take(formats)
                            .map(|f| (*f).to_owned())
                            .collect(),
                        language: "en".to_owned(),
                        created_at: now,
                        app_version: "2.0.0".to_owned(),
                        members,
                    })?;
                    Ok((report, tx.reports(scope).parts(report)?))
                })
                .await
                .map(|(report, parts)| {
                    ids.reports.push(report);
                    ids.parts.extend(parts);
                })
        }
        Op::Part(i, e) => match pick(&ids.parts, i) {
            Some(id) => store
                .write(move |tx| tx.reports(scope).apply_part(id, PART_EVENTS[e], now))
                .await
                .map(drop),
            None => Ok(()),
        },
        Op::Stale(i, stale) => match pick(&ids.reports, i) {
            Some(id) => {
                store
                    .write(move |tx| tx.reports(scope).set_stale(id, stale))
                    .await
            }
            None => Ok(()),
        },
        Op::Close => {
            store
                .write(move |tx| tx.batches(scope).close(batch, ClosureReason::Manual, now))
                .await
        }
    };
    if let Err(error) = result {
        assert!(
            matches!(
                error,
                mivia_ports::store::StoreError::Tx(TxError::Illegal(_))
            ),
            "{error:?}"
        );
    }
}

const RECOUNT: &str = "SELECT
  b.files - (SELECT count(*) FROM source_file WHERE batch_id = b.id),
  b.done - (SELECT count(*) FROM computation WHERE batch_id = b.id AND state = 'done'),
  b.failed - (SELECT count(*) FROM computation WHERE batch_id = b.id AND state = 'failed'),
  b.problems - (SELECT count(*) FROM source_file WHERE batch_id = b.id AND state IN ('rejected', 'unreadable'))
             - (SELECT count(*) FROM computation WHERE batch_id = b.id AND state IN ('failed', 'unavailable', 'lost')),
  b.active_count - (SELECT count(*) FROM computation WHERE batch_id = b.id AND state IN ('queued', 'submitting', 'computing')),
  b.reports_total - (SELECT count(*) FROM report_part p JOIN report r ON r.id = p.report_id WHERE r.batch_id = b.id),
  b.reports_saved - (SELECT count(*) FROM report_part p JOIN report r ON r.id = p.report_id WHERE r.batch_id = b.id AND p.state = 'saved'),
  b.reports_problem - (SELECT count(*) FROM report_part p JOIN report r ON r.id = p.report_id WHERE r.batch_id = b.id AND p.state IN ('not_saved', 'failed')),
  b.reports_stale - (SELECT count(*) FROM report_part p JOIN report r ON r.id = p.report_id WHERE r.batch_id = b.id AND r.stale = 1 AND r.replaced_by IS NULL),
  (SELECT count(*) FROM batch_model_choice WHERE batch_id = b.id AND sort_at <> b.sort_at),
  b.problems > 0 AND b.needs_attention = 0
FROM batch b";

fn drift(db: &std::path::Path) -> Vec<Vec<i64>> {
    let conn = Connection::open(db).unwrap();
    let mut statement = conn.prepare(RECOUNT).unwrap();
    statement
        .query_map([], |row| (0..11).map(|i| row.get(i)).collect())
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

proptest! {
    #![proptest_config(Config { cases: 32, max_shrink_iters: 256, ..Config::default() })]

    #[test]
    fn batch_counters_equal_recount(ops in prop::collection::vec(op(), 1..20)) {
        let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let rows = runtime.block_on(async {
            let fx = fixture(StoreConfig::for_tests()).await;
            let (scope, batch) = seed(&fx.store, |tx| {
                let scope = account(tx, "a");
                let batch = tx.batches(scope).open(&new_batch("S")).unwrap();
                (scope, batch)
            })
            .await;
            let mut ids = Ids::default();
            for op in ops {
                run(&fx.store, scope, batch, op, &mut ids).await;
            }
            let rows = drift(&fx.db());
            drop(fx.store);
            rows
        });
        prop_assert_eq!(rows, vec![vec![0; 11]]);
    }
}

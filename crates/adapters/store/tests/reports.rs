use mivia_domain::batch::ClosureReason;
use mivia_domain::computation::{ComputationEvent, FailReason, JobOutcome};
use mivia_domain::ids::{BatchId, ComputationId, Fingerprint, ReportPartId, Timestamp};
use mivia_domain::label::LabelRule;
use mivia_domain::naming::Language;
use mivia_domain::output::{Format, ReportFolder};
use mivia_domain::path::WinPath;
use mivia_domain::report::{OmittedReason, PartFailure, PartState, SaveError, SaveProblem};
use mivia_domain::source_file::SourceFileEvent;
use mivia_ports::fs::Staged;
use mivia_ports::queries::Queries;
use mivia_ports::store::pipeline::JobRecord;
use mivia_ports::store::reports::{ReportSpec, Rerequest};
use mivia_ports::store::{
    AccountScope, NewBatch, NewComputation, NewFolder, NewSourceFile, Origin, WriteTx,
};
use mivia_store::StoreConfig;

use crate::common::{account, fixture, new_batch, seed};

const NOW: Timestamp = Timestamp(10_000);

fn spec(formats: Vec<Format>) -> ReportSpec {
    ReportSpec {
        formats,
        language: Language::De,
        decimal_comma: true,
        include_images: false,
        timezone: "Europe/Berlin".to_owned(),
    }
}

fn folder_batch(tx: &mut dyn WriteTx, scope: AccountScope) -> BatchId {
    let folder = tx
        .folders(scope)
        .add(&NewFolder {
            path: WinPath::parse(r"D:\Lab").unwrap(),
            position: 0,
            quiet_period_ms: 1_000,
            label_rule: LabelRule::Auto,
            report_settings_json: "{}".to_owned(),
            report_folder: ReportFolder::Subfolder("MiViA Berichte".to_owned()),
            model_choices: Vec::new(),
        })
        .unwrap();
    tx.batches(scope)
        .open(&NewBatch {
            origin: Origin::Folder(folder),
            ..new_batch("Run")
        })
        .unwrap()
}

fn add(tx: &mut dyn WriteTx, scope: AccountScope, batch: BatchId, name: &str) -> ComputationId {
    let file = tx
        .source_files(scope)
        .add(&NewSourceFile {
            batch_id: batch,
            path: WinPath::parse(&format!(r"D:\Lab\{name}")).unwrap(),
            label: name.to_owned(),
            size: 10,
            mtime: Timestamp(1_000),
            next_attempt_at: Timestamp(1_000),
        })
        .unwrap();
    for event in [
        SourceFileEvent::ReadinessPassed,
        SourceFileEvent::UploadClaimed,
        SourceFileEvent::UploadSucceeded,
    ] {
        tx.source_files(scope).apply(file, event, NOW).unwrap();
    }
    tx.computations(scope)
        .add(&NewComputation {
            batch_id: batch,
            source_file_id: file,
            model_choice_key: "grain:".to_owned(),
            model_version: None,
            app_version: "2.0.0".to_owned(),
            next_attempt_at: Timestamp(1_000),
            reused_from: None,
        })
        .unwrap()
}

fn finish(tx: &mut dyn WriteTx, scope: AccountScope, id: ComputationId, outcome: JobOutcome) {
    tx.computations(scope)
        .apply(id, ComputationEvent::SubmitClaimed, NOW)
        .unwrap();
    let job = JobRecord {
        job_id: format!("job-{}", id.0),
        outcome,
        result_id: None,
        submitted_at: NOW,
        next_poll_at: NOW,
    };
    tx.pipeline(scope).record_job(id, &job, NOW).unwrap();
}

const DONE: JobOutcome = JobOutcome::Done { reused: false };

fn closed(tx: &mut dyn WriteTx, scope: AccountScope, batch: BatchId) {
    tx.batches(scope)
        .close(batch, ClosureReason::Manual, Timestamp(5_000))
        .unwrap();
}

#[tokio::test]
async fn closing_waits_for_terminal_computations_then_builds_in_natural_order() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let outcome = seed(&fx.store, |tx| {
        let scope = account(tx, "a");
        let batch = folder_batch(tx, scope);
        let ids: Vec<ComputationId> = ["A10.tif", "a2.tif", "A1.tif", "bad.tif"]
            .iter()
            .map(|name| add(tx, scope, batch, name))
            .collect();
        closed(tx, scope, batch);
        let pending = tx.reports(scope).waiting().unwrap();
        for id in &ids[..3] {
            finish(tx, scope, *id, DONE);
        }
        finish(
            tx,
            scope,
            ids[3],
            JobOutcome::Failed(FailReason::ServerFailed),
        );
        let ready = tx.reports(scope).waiting().unwrap();
        let report = ready[0].id;
        let parts = tx
            .reports(scope)
            .build(report, &spec(vec![Format::Csv, Format::Pdf]), NOW)
            .unwrap();
        let first = tx.reports(scope).next_request(NOW).unwrap().unwrap();
        let left = tx.reports(scope).waiting().unwrap();
        (
            pending[0].pending,
            ready[0].pending,
            parts,
            first,
            left.len(),
        )
    })
    .await;
    let (pending, ready, parts, first, left) = outcome;
    assert!(pending && !ready);
    assert_eq!((parts, left), (2, 0));
    assert_eq!(first.format, Format::Pdf);
    assert_eq!(first.part, (1, 1));
    let labels: Vec<&str> = first.members.iter().map(|(_, l)| l.as_str()).collect();
    assert_eq!(labels, ["A1.tif", "a2.tif", "A10.tif"]);
    assert_eq!(
        first.omitted,
        [("bad.tif".to_owned(), OmittedReason::Failed)]
    );
    assert!(first.decimal_comma && first.timezone == "Europe/Berlin");
    fx.store.close().await.unwrap();
}

#[tokio::test]
async fn a_report_without_done_images_has_nothing_to_report() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let parts = seed(&fx.store, |tx| {
        let scope = account(tx, "a");
        let batch = folder_batch(tx, scope);
        let id = add(tx, scope, batch, "a.tif");
        tx.computations(scope)
            .apply(id, ComputationEvent::Cancel, NOW)
            .unwrap();
        closed(tx, scope, batch);
        let report = tx.reports(scope).waiting().unwrap()[0].id;
        let parts = tx
            .reports(scope)
            .build(report, &spec(vec![Format::Pdf]), NOW)
            .unwrap();
        (parts, tx.reports(scope).next_request(NOW).unwrap())
    })
    .await;
    assert_eq!(parts, (0, None));
    fx.store.close().await.unwrap();
}

fn built_report(tx: &mut dyn WriteTx, scope: AccountScope, files: usize) -> Vec<ReportPartId> {
    let batch = folder_batch(tx, scope);
    for n in 0..files {
        let id = add(tx, scope, batch, &format!("img_{n:04}.tif"));
        finish(tx, scope, id, DONE);
    }
    closed(tx, scope, batch);
    let report = tx.reports(scope).waiting().unwrap()[0].id;
    tx.reports(scope)
        .build(report, &spec(vec![Format::Pdf]), NOW)
        .unwrap();
    tx.reports(scope).parts(report).unwrap()
}

#[tokio::test]
async fn one_request_is_in_flight_and_retries_wait() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let seen = seed(&fx.store, |tx| {
        let scope = account(tx, "a");
        let parts = built_report(tx, scope, 150);
        let repo = tx.reports(scope);
        let first = repo.next_request(NOW).unwrap().unwrap().id;
        repo.request_retry(first, NOW.plus_ms(30_000), 0).unwrap();
        let second = repo.next_request(NOW).unwrap().unwrap().id;
        repo.requested(second, "srv-2", NOW).unwrap();
        let blocked = repo.in_flight().unwrap();
        let later = repo
            .next_request(NOW.plus_ms(30_000))
            .unwrap()
            .map(|r| r.id);
        (parts, first, second, blocked, later)
    })
    .await;
    let (parts, first, second, blocked, later) = seen;
    assert_eq!((first, second), (parts[0], parts[1]));
    assert!(blocked);
    assert_eq!(later, Some(parts[0]));
    fx.store.close().await.unwrap();
}

#[tokio::test]
async fn a_too_large_part_splits_once_and_renumbers_the_rest() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let seen = seed(&fx.store, |tx| {
        let scope = account(tx, "a");
        let parts = built_report(tx, scope, 250);
        let repo = tx.reports(scope);
        repo.requested(parts[1], "srv", NOW).unwrap();
        let state = repo.split(parts[1], NOW).unwrap();
        let mut order = Vec::new();
        while let Some(request) = repo.next_request(NOW).unwrap() {
            order.push((request.part, request.members.len()));
            repo.requested(request.id, "srv", NOW).unwrap();
            repo.rerequest(request.id, Rerequest::Expired, NOW).unwrap();
            repo.request_retry(request.id, NOW.plus_ms(1), 0).unwrap();
        }
        let again = repo.split(parts[1], NOW).unwrap();
        (state, order, again)
    })
    .await;
    let (state, order, again) = seen;
    assert_eq!(state, PartState::Requested);
    assert_eq!(
        order,
        [((1, 4), 100), ((2, 4), 50), ((3, 4), 50), ((4, 4), 50)]
    );
    assert_eq!(again, PartState::Failed(PartFailure::TooLarge));
    fx.store.close().await.unwrap();
}

#[tokio::test]
async fn lost_members_leave_the_part_and_join_the_omitted_list() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let seen = seed(&fx.store, |tx| {
        let scope = account(tx, "a");
        let parts = built_report(tx, scope, 3);
        let repo = tx.reports(scope);
        let jobs: Vec<String> = repo.next_request(NOW).unwrap().unwrap().members[..2]
            .iter()
            .map(|(job, _)| job.clone())
            .collect();
        let left = repo.members_lost(parts[0], &jobs).unwrap();
        let after = repo.next_request(NOW).unwrap().unwrap();
        (left, after)
    })
    .await;
    let (left, after) = seen;
    assert_eq!(left, 1);
    assert_eq!(after.members.len(), 1);
    assert_eq!(after.omitted.len(), 2);
    assert!(after.omitted.iter().all(|(_, r)| *r == OmittedReason::Lost));
    fx.store.close().await.unwrap();
}

#[tokio::test]
async fn downloaded_parts_carry_what_saving_needs() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let seen = seed(&fx.store, |tx| {
        let scope = account(tx, "a");
        let parts = built_report(tx, scope, 1);
        let repo = tx.reports(scope);
        repo.requested(parts[0], "srv", NOW).unwrap();
        let staging = WinPath::parse(r"C:\App\reports\staging\1.pdf").unwrap();
        let staged = Staged {
            size: 42,
            sha256: Fingerprint([7; 32]),
            head: b"%PDF-".to_vec(),
        };
        repo.downloaded(parts[0], &staging, &staged).unwrap();
        let free = !repo.in_flight().unwrap();
        let row = repo.save_due(NOW).unwrap().remove(0);
        let failed = repo
            .save_failed(parts[0], SaveError::Retryable(SaveProblem::Busy), NOW)
            .unwrap();
        let soon = repo.save_due(NOW.plus_ms(1_000)).unwrap().len();
        let later = repo.next_save().unwrap();
        let target = WinPath::parse(r"D:\Lab\MiViA Berichte\r.pdf").unwrap();
        repo.saved(parts[0], &target, NOW).unwrap();
        let done = repo.save_due(NOW.plus_ms(3_600_000)).unwrap().len();
        (free, row, failed, soon, later, done)
    })
    .await;
    let (free, row, failed, soon, later, done) = seen;
    assert!(free);
    assert_eq!(row.root, Some(WinPath::parse(r"D:\Lab").unwrap()));
    assert_eq!(
        row.report_folder,
        Some(ReportFolder::Subfolder("MiViA Berichte".to_owned()))
    );
    assert_eq!(
        (row.size, row.sha256, row.label.as_str()),
        (42, Fingerprint([7; 32]), "Run")
    );
    assert_eq!(row.analysis_json, r#"{"en":"grain","de":"grain"}"#);
    assert_eq!(row.closed_at, Timestamp(5_000));
    assert_eq!(failed, PartState::NotSaved);
    assert_eq!((soon, later), (0, Some(NOW.plus_ms(300_000))));
    assert_eq!(done, 0);
    fx.store.close().await.unwrap();
}

#[tokio::test]
async fn running_an_image_again_marks_its_saved_report_stale() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let stale = seed(&fx.store, |tx| {
        let scope = account(tx, "a");
        let batch = folder_batch(tx, scope);
        let id = add(tx, scope, batch, "a.tif");
        finish(tx, scope, id, DONE);
        closed(tx, scope, batch);
        let report = tx.reports(scope).waiting().unwrap()[0].id;
        tx.reports(scope)
            .build(report, &spec(vec![Format::Pdf]), NOW)
            .unwrap();
        let part = tx.reports(scope).parts(report).unwrap()[0];
        let target = WinPath::parse(r"D:\Lab\r.pdf").unwrap();
        tx.reports(scope).saved(part, &target, NOW).unwrap();
        tx.computations(scope)
            .apply(id, ComputationEvent::RunAgain, NOW)
            .unwrap();
        batch
    })
    .await;
    let view = fx
        .store
        .get_batch(AccountScope(mivia_domain::ids::AccountId(1)), stale)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(view.summary.counts.reports_stale, 1);
    assert_eq!(view.analyses[0].key, "grain:");
    let reports = fx
        .store
        .list_series_reports(AccountScope(mivia_domain::ids::AccountId(1)), stale)
        .await
        .unwrap()
        .reports;
    assert!(reports[0].stale);
    let part = &reports[0].parts[0];
    assert_eq!(
        (part.state.as_str(), part.file_name.as_deref()),
        ("saved", Some("r.pdf"))
    );
    fx.store.close().await.unwrap();
}

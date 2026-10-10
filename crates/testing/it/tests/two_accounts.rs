use std::sync::Arc;

use mivia_application as _;
use mivia_domain::batch::ClosureReason;
use mivia_domain::computation::ComputationEvent;
use mivia_domain::hold::{Access, Gate, HoldReason};
use mivia_domain::ids::{BatchId, ComputationId, ModelChoice, Timestamp};
use mivia_domain::label::LabelRule;
use mivia_domain::output::ReportFolder;
use mivia_domain::path::WinPath;
use mivia_ports::queries::Queries;
use mivia_ports::store::{
    AccountScope, BatchModelChoice, NewAccount, NewBatch, NewComputation, NewFolder, NewSourceFile,
    Origin, StoreError, StoreExt, TxError, WriteTx,
};
use mivia_store::{SqliteStore, StoreConfig};
use mivia_testkit::observer::RecordingObserver;

struct Seeded {
    scope: AccountScope,
    closed: BatchId,
    open: BatchId,
    computation: ComputationId,
}

fn win(text: &str) -> WinPath {
    WinPath::parse(text).unwrap()
}

fn seed_account(tx: &mut dyn WriteTx, name: &str) -> Result<Seeded, TxError> {
    let id = tx.accounts().upsert(&NewAccount {
        server_url: "https://app.mivia.ai".to_owned(),
        server_account_id: name.to_owned(),
        display_name: name.to_owned(),
        email: String::new(),
    })?;
    let scope = AccountScope(id);
    let folder = tx.folders(scope).add(&NewFolder {
        path: win(&format!(r"D:\{name}")),
        position: 0,
        quiet_period_ms: 2_000,
        label_rule: LabelRule::Auto,
        report_settings_json: "{}".to_owned(),
        report_folder: ReportFolder::BesideSeries,
        model_choices: Vec::new(),
    })?;
    let batch = |label: &str| NewBatch {
        origin: Origin::Folder(folder),
        batch_key: format!("{name}-{label}"),
        label: format!("{name} {label}"),
        opened_at: Timestamp(1_000),
        report_settings_json: "{}".to_owned(),
        app_version: "2.0.0".to_owned(),
        model_choices: vec![BatchModelChoice {
            choice: ModelChoice {
                model_id: "grain".to_owned(),
                customization_id: None,
            },
            names_json: "{}".to_owned(),
            model_version: None,
        }],
    };
    let closed = tx.batches(scope).open(&batch("closed"))?;
    let open = tx.batches(scope).open(&batch("open"))?;
    let file = tx.source_files(scope).add(&NewSourceFile {
        batch_id: closed,
        path: win(&format!(r"D:\{name}\a.tif")),
        label: "a.tif".to_owned(),
        size: 1,
        mtime: Timestamp(1_000),
        next_attempt_at: Timestamp(1_000),
    })?;
    let computation = tx.computations(scope).add(&NewComputation {
        batch_id: closed,
        source_file_id: file,
        model_choice_key: "grain:".to_owned(),
        model_version: None,
        app_version: "2.0.0".to_owned(),
        next_attempt_at: Timestamp(1_000),
        reused_from: None,
    })?;
    tx.batches(scope)
        .close(closed, ClosureReason::QuietPeriod, Timestamp(4_000))?;
    Ok(Seeded {
        scope,
        closed,
        open,
        computation,
    })
}

#[tokio::test]
async fn two_accounts_share_one_file_but_never_see_each_other() {
    let dir = tempfile::tempdir().unwrap();
    let observer = Arc::new(RecordingObserver::default());
    let store = SqliteStore::open(
        &dir.path().join("mivia.db"),
        StoreConfig::for_tests(),
        Arc::clone(&observer) as _,
    )
    .await
    .unwrap();
    let (a, b) = store
        .write(|tx| {
            let a = seed_account(tx, "alpha")?;
            let b = seed_account(tx, "beta")?;
            tx.accounts().activate(a.scope.0)?;
            tx.holds(b.scope)
                .raise(HoldReason::Offline, Timestamp(1), None)?;
            Ok((a, b))
        })
        .await
        .unwrap();

    let history = store.list_history(a.scope, None).await.unwrap();
    let labels: Vec<_> = history
        .head
        .iter()
        .chain(&history.rows)
        .map(|row| row.label.as_str())
        .collect();
    assert_eq!(labels, ["alpha open", "alpha closed"]);
    assert_eq!(store.get_batch(a.scope, b.closed).await.unwrap(), None);
    assert!(store.get_batch(b.scope, b.open).await.unwrap().is_some());
    let foreign_items = store.list_batch_items(a.scope, b.closed, None).await;
    assert_eq!(foreign_items.unwrap().items, []);
    let folders = store.list_folders(a.scope).await.unwrap();
    let paths: Vec<_> = folders.folders.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, [r"D:\alpha"]);

    let (due_a, access_a, access_b) = store
        .write(move |tx| {
            let due = tx
                .computations(a.scope)
                .due_for_submit(Timestamp(9_000), &[])?;
            let access_a = tx.holds(a.scope).access(Gate::Upload)?;
            let access_b = tx.holds(b.scope).access(Gate::Upload)?;
            Ok((due, access_a, access_b))
        })
        .await
        .unwrap();
    assert_eq!(due_a, [a.computation]);
    assert_eq!((access_a, access_b), (Access::Allowed, Access::Blocked));

    let foreign_write = store
        .write(move |tx| {
            tx.computations(a.scope).apply(
                b.computation,
                ComputationEvent::Cancel,
                Timestamp(9_000),
            )
        })
        .await;
    assert_eq!(foreign_write, Err(StoreError::Tx(TxError::NotFound)));
    let last = observer.commits().last().cloned().unwrap();
    assert!(last.revision > 0);
}

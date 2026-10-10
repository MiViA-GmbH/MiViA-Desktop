use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

use mivia_domain::batch::ClosureReason;
use mivia_domain::ids::{AccountId, BatchId, ModelChoice, Timestamp};
use mivia_domain::path::WinPath;
use mivia_ports::store::{
    AccountScope, BatchModelChoice, CommitObserver, Committed, NewAccount, NewBatch, NewSourceFile,
    Origin, StoreExt, WriteTx,
};
use mivia_store::{SqliteStore, StoreConfig};
use tempfile::TempDir;

#[derive(Default)]
pub(crate) struct Recorder {
    pub(crate) commits: AtomicUsize,
    pub(crate) revision: AtomicI64,
}

impl CommitObserver for Recorder {
    fn committed(&self, committed: &Committed) {
        self.commits.fetch_add(1, Ordering::SeqCst);
        self.revision.store(committed.revision, Ordering::SeqCst);
    }
}

pub(crate) struct Fixture {
    pub(crate) dir: TempDir,
    pub(crate) store: SqliteStore,
    pub(crate) recorder: Arc<Recorder>,
}

impl Fixture {
    pub(crate) fn db(&self) -> PathBuf {
        db_path(self.dir.path())
    }
}

pub(crate) fn db_path(dir: &Path) -> PathBuf {
    dir.join("mivia.db")
}

pub(crate) async fn fixture(config: StoreConfig) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let recorder = Arc::new(Recorder::default());
    let store = SqliteStore::open(&db_path(dir.path()), config, Arc::clone(&recorder) as _)
        .await
        .unwrap();
    Fixture {
        dir,
        store,
        recorder,
    }
}

pub(crate) fn account(tx: &mut dyn WriteTx, name: &str) -> AccountScope {
    let id: AccountId = tx
        .accounts()
        .upsert(&NewAccount {
            server_url: "https://app.mivia.ai".to_owned(),
            server_account_id: name.to_owned(),
            display_name: name.to_owned(),
        })
        .unwrap();
    AccountScope(id)
}

pub(crate) fn new_batch(label: &str) -> NewBatch {
    NewBatch {
        origin: Origin::OneOff,
        batch_key: label.to_owned(),
        label: label.to_owned(),
        opened_at: Timestamp(1_000),
        report_settings_json: "{}".to_owned(),
        app_version: "2.0.0".to_owned(),
        model_choices: vec![BatchModelChoice {
            choice: ModelChoice {
                model_id: "grain".to_owned(),
                customization_id: None,
            },
            names_json: "{}".to_owned(),
            model_version: Some("1".to_owned()),
        }],
    }
}

pub(crate) fn new_file(batch_id: BatchId, name: &str) -> NewSourceFile {
    NewSourceFile {
        batch_id,
        path: WinPath::parse(&format!(r"D:\Lab\{name}")).unwrap(),
        label: name.to_owned(),
        size: 10,
        mtime: Timestamp(1_000),
        next_attempt_at: Timestamp(1_000),
    }
}

pub(crate) fn closed_batch(tx: &mut dyn WriteTx, scope: AccountScope, label: &str) -> BatchId {
    let id = tx.batches(scope).open(&new_batch(label)).unwrap();
    tx.batches(scope)
        .close(id, ClosureReason::Manual, Timestamp(5_000))
        .unwrap();
    id
}

pub(crate) async fn seed<R: Send + 'static>(
    store: &SqliteStore,
    work: impl FnOnce(&mut dyn WriteTx) -> R + Send + 'static,
) -> R {
    store.write(move |tx| Ok(work(tx))).await.unwrap()
}

use std::sync::Arc;

use mivia_application::intake::{
    AddFolder, Intake, IntakeSink, MarkerSelf, Ports, SIGNAL_CAPACITY, Tick,
};
use mivia_application::report_settings::ReportSettings;
use mivia_domain::ids::{BatchId, FolderId, ModelChoice, Timestamp};
use mivia_domain::label::LabelRule;
use mivia_domain::output::ReportFolder;
use mivia_domain::path::WinPath;
use mivia_ports::clock::Clock;
use mivia_ports::ipc::history::HistoryFilter;
use mivia_ports::ipc::views::{BatchItem, BatchSummary};
use mivia_ports::queries::Queries;
use mivia_ports::store::{AccountScope, NewAccount, StoreExt};
use mivia_ports::watch::SignalKind;
use mivia_store::{SqliteStore, StoreConfig};
use mivia_testkit::clock::ManualClock;
use mivia_testkit::fs::InMemoryFs;
use mivia_testkit::observer::RecordingObserver;
use mivia_testkit::telemetry::RecordingTelemetry;
use mivia_testkit::watch::FakeFolderWatcher;
use tempfile::TempDir;

pub(crate) const START: Timestamp = Timestamp(1_800_000_000_000);
pub(crate) const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3, 0xFF, 0xD9];
const SETTLE_ROUNDS: usize = 200;

pub(crate) fn win(text: &str) -> WinPath {
    WinPath::parse(text).unwrap()
}

pub(crate) fn jpeg(seed: u8) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xD8, 0xFF, 0xE0, seed];
    bytes.extend([0xFF, 0xD9]);
    bytes
}

pub(crate) fn choice(model: &str) -> ModelChoice {
    ModelChoice {
        model_id: model.to_owned(),
        customization_id: None,
    }
}

pub(crate) struct World {
    pub(crate) store: Arc<SqliteStore>,
    pub(crate) fs: Arc<InMemoryFs>,
    pub(crate) watcher: Arc<FakeFolderWatcher>,
    pub(crate) clock: Arc<ManualClock>,
    pub(crate) intake: Intake,
    pub(crate) scope: AccountScope,
    pub(crate) telemetry: Arc<RecordingTelemetry>,
    _dir: TempDir,
}

impl World {
    pub(crate) async fn new() -> Self {
        Self::with_marker(None).await
    }

    pub(crate) async fn with_marker(marker: Option<MarkerSelf>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(
                &dir.path().join("mivia.db"),
                StoreConfig::for_tests(),
                Arc::new(RecordingObserver::default()),
            )
            .await
            .unwrap(),
        );
        let scope = store
            .write(|tx| {
                let id = tx.accounts().upsert(&NewAccount {
                    server_url: "https://app.mivia.ai".to_owned(),
                    server_account_id: "lab".to_owned(),
                    display_name: "Lab".to_owned(),
                    email: String::new(),
                })?;
                tx.accounts().activate(id)?;
                Ok(AccountScope(id))
            })
            .await
            .unwrap();
        let fs = Arc::new(InMemoryFs::windows());
        fs.set_now(START);
        fs.mkdir(&win(r"D:\Lab"));
        let clock = Arc::new(ManualClock::new(START));
        let telemetry = Arc::new(RecordingTelemetry::default());
        let (sink, receiver) = IntakeSink::channel(SIGNAL_CAPACITY);
        let watcher = Arc::new(FakeFolderWatcher::new(
            Arc::clone(&sink) as _,
            Arc::clone(&fs),
        ));
        let ports = Ports {
            store: Arc::clone(&store) as _,
            fs: Arc::clone(&fs) as _,
            watcher: Arc::clone(&watcher) as _,
            clock: Arc::clone(&clock) as _,
            telemetry: Arc::clone(&telemetry) as _,
        };
        let intake = Intake::new(ports, scope, "2.0.0", (sink, receiver)).with_marker(marker);
        Self {
            store,
            fs,
            watcher,
            clock,
            intake,
            scope,
            telemetry,
            _dir: dir,
        }
    }

    pub(crate) async fn add(&self, quiet_ms: i64, include_existing: bool) -> FolderId {
        self.add_with(quiet_ms, include_existing, ReportSettings::default())
            .await
    }

    pub(crate) async fn add_with(
        &self,
        quiet_ms: i64,
        include_existing: bool,
        report_settings: ReportSettings,
    ) -> FolderId {
        self.add_at(r"D:\Lab", quiet_ms, include_existing, report_settings)
            .await
    }

    pub(crate) async fn add_at(
        &self,
        path: &str,
        quiet_ms: i64,
        include_existing: bool,
        report_settings: ReportSettings,
    ) -> FolderId {
        self.intake
            .add_folder(AddFolder {
                path: win(path),
                model_choices: vec![choice("grain"), choice("phase")],
                quiet_ms,
                label_rule: LabelRule::Auto,
                include_existing,
                report_settings,
                report_folder: ReportFolder::Subfolder("MiViA reports".to_owned()),
            })
            .await
            .unwrap()
    }

    pub(crate) async fn finish(self) {
        self.store.close().await.unwrap();
    }

    pub(crate) fn drop_file(&self, path: &str, bytes: &[u8]) {
        let path = win(path);
        self.fs.put(&path, bytes);
        self.watcher.emit(SignalKind::Touched(path));
    }

    pub(crate) async fn settle(&self) {
        for _ in 0..SETTLE_ROUNDS {
            let ticks = [
                self.intake.intake_tick().await.unwrap(),
                self.intake.readiness_tick().await.unwrap(),
                self.intake.closer_tick().await.unwrap(),
            ];
            if !ticks.contains(&Tick::Busy) {
                return;
            }
        }
        panic!("intake did not settle");
    }

    pub(crate) async fn pass(&self, ms: i64) {
        self.settle().await;
        self.clock.advance(ms);
        self.fs.set_now(self.clock.now());
        self.settle().await;
    }

    pub(crate) async fn series(&self) -> Vec<BatchSummary> {
        let page = self
            .store
            .list_history(self.scope, HistoryFilter::default(), None)
            .await
            .unwrap();
        page.head
            .into_iter()
            .chain(page.rows)
            .map(|row| row.summary)
            .collect()
    }

    pub(crate) async fn items(&self, batch: &BatchSummary) -> Vec<BatchItem> {
        let id = BatchId(batch.id.parse().unwrap());
        let page = self.store.list_batch_items(self.scope, id, None).await;
        page.unwrap().items
    }

    pub(crate) async fn states(&self, batch: &BatchSummary) -> Vec<(String, String)> {
        self.items(batch)
            .await
            .into_iter()
            .map(|item| (item.label, item.state))
            .collect()
    }
}

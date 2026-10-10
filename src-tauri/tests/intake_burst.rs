use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mivia_application::intake::{
    AddFolder, Intake, IntakeError, IntakeSink, Ports, SIGNAL_CAPACITY, Tick, run,
};
use mivia_application::report_settings::ReportSettings;
use mivia_domain::ids::{MINUTE_MS, ModelChoice};
use mivia_domain::label::LabelRule;
use mivia_domain::output::ReportFolder;
use mivia_domain::path::WinPath;
use mivia_os::{NotifyWatcher, OsFileSystem, SystemClock};
use mivia_ports::queries::Queries;
use mivia_ports::store::{AccountScope, NewAccount, StoreExt};
use mivia_store::{SqliteStore, StoreConfig};
use mivia_testkit::observer::RecordingObserver;

const FILES: usize = 5_000;
const FILE_BYTES: usize = 1024 * 1024;
const WRITERS: usize = 8;
const READY_BUDGET: Duration = Duration::from_secs(30);
const CHANNEL_BUDGET: usize = 32_768;

struct RemoveOnDrop(PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.0));
    }
}

fn image(index: usize) -> Vec<u8> {
    let mut bytes = vec![0x5A; FILE_BYTES];
    bytes[..4].copy_from_slice(&[0xFF, 0xD8, 0xFF, 0xE0]);
    bytes[4..12].copy_from_slice(&(index as u64).to_le_bytes());
    bytes[FILE_BYTES - 2..].copy_from_slice(&[0xFF, 0xD9]);
    bytes
}

fn write_burst(root: &Path) -> Duration {
    let started = Instant::now();
    let handles: Vec<_> = (0..WRITERS)
        .map(|writer| {
            let root = root.to_path_buf();
            std::thread::spawn(move || {
                for index in (writer..FILES).step_by(WRITERS) {
                    std::fs::write(root.join(format!("img{index:05}.jpg")), image(index)).unwrap();
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    started.elapsed()
}

async fn open_store(dir: &Path) -> (Arc<SqliteStore>, AccountScope) {
    let store = SqliteStore::open(
        &dir.join("mivia.db"),
        StoreConfig::production(),
        Arc::new(RecordingObserver::default()),
    )
    .await
    .unwrap();
    let scope = store
        .write(|tx| {
            let id = tx.accounts().upsert(&NewAccount {
                server_url: "https://app.mivia.ai".to_owned(),
                server_account_id: "burst".to_owned(),
                display_name: "Burst".to_owned(),
            })?;
            tx.accounts().activate(id)?;
            Ok(AccountScope(id))
        })
        .await
        .unwrap();
    (Arc::new(store), scope)
}

#[derive(Clone, Copy)]
enum Worker {
    Intake,
    Readiness,
    Closer,
}

async fn tick(intake: Arc<Intake>, worker: Worker) -> Result<Tick, IntakeError> {
    match worker {
        Worker::Intake => intake.intake_tick().await,
        Worker::Readiness => intake.readiness_tick().await,
        Worker::Closer => intake.closer_tick().await,
    }
}

fn spawn(intake: &Arc<Intake>, worker: Worker) -> tokio::task::JoinHandle<()> {
    let intake = Arc::clone(intake);
    tokio::spawn(async move {
        let wake = match worker {
            Worker::Intake => &intake.wakers.intake,
            Worker::Readiness => &intake.wakers.readiness,
            Worker::Closer => &intake.wakers.closer,
        };
        run("burst", &SystemClock, wake, || {
            tick(Arc::clone(&intake), worker)
        })
        .await;
    })
}

async fn settled(store: &SqliteStore, scope: AccountScope) -> (u32, bool) {
    let page = store.list_history(scope, None).await.unwrap();
    let files = page
        .head
        .iter()
        .chain(&page.rows)
        .map(|s| s.counts.files)
        .sum();
    let detected = store
        .write(move |tx| tx.intake(scope).next_probe_at())
        .await
        .unwrap()
        .is_some();
    (files, !detected)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn burst_of_5000_files_loses_nothing() {
    let base = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let watched = base.path().join("watched");
    std::fs::create_dir(&watched).unwrap();
    let watched = std::fs::canonicalize(&watched).unwrap();
    let _images = RemoveOnDrop(watched.clone());
    let (store, scope) = open_store(base.path()).await;
    let (sink, receiver) = IntakeSink::channel(SIGNAL_CAPACITY);
    let ports = Ports {
        store: Arc::clone(&store) as _,
        fs: Arc::new(OsFileSystem),
        watcher: Arc::new(NotifyWatcher::new(Arc::clone(&sink) as _)),
        clock: Arc::new(SystemClock),
    };
    let intake = Arc::new(Intake::new(ports, scope, "2.0.0", (sink, receiver)));
    intake
        .add_folder(AddFolder {
            path: WinPath::parse(watched.to_str().unwrap()).unwrap(),
            model_choices: vec![ModelChoice {
                model_id: "grain".to_owned(),
                customization_id: None,
            }],
            quiet_ms: MINUTE_MS,
            label_rule: LabelRule::Auto,
            include_existing: false,
            report_settings: ReportSettings::default(),
            report_folder: ReportFolder::Subfolder("MiViA reports".to_owned()),
        })
        .await
        .unwrap();
    let workers: Vec<_> = [
        Worker::Readiness,
        Worker::Readiness,
        Worker::Closer,
        Worker::Intake,
    ]
    .into_iter()
    .map(|worker| spawn(&intake, worker))
    .collect();

    let writing = write_burst(&watched);
    let written = Instant::now();
    let (mut files, mut done) = (0, false);
    while written.elapsed() < Duration::from_secs(180) && !(done && files as usize == FILES) {
        tokio::time::sleep(Duration::from_millis(200)).await;
        (files, done) = settled(&store, scope).await;
    }
    let ready_after = written.elapsed();
    let report = format!(
        "{{\"files\":{FILES},\"writeMs\":{},\"lost\":{},\"readyMs\":{},\"channelHighWater\":{}}}",
        writing.as_millis(),
        i64::try_from(FILES).unwrap() - i64::from(files),
        ready_after.as_millis(),
        intake.high_water()
    );
    std::fs::write(
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("burst-5000.json"),
        &report,
    )
    .unwrap();
    assert_eq!(files as usize, FILES, "{report}");
    assert!(done, "{report}");
    let optimized = !cfg!(debug_assertions);
    assert!(!optimized || ready_after <= READY_BUDGET, "{report}");
    assert!(intake.high_water() < CHANNEL_BUDGET, "{report}");
    for worker in workers {
        worker.abort();
        drop(worker.await);
    }
    store.close().await.unwrap();
    drop((intake, store));
}

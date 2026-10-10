use std::collections::BTreeMap;
use std::fs::File;
use std::os::windows::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use mivia_domain::path::WinPath;
use mivia_os::{NotifyWatcher, OsFileSystem, SystemClock};
use mivia_ports::clock::Clock;
use mivia_ports::fs::FileSystem;
use mivia_ports::watch::{FolderWatcher, FsSignal};
use mivia_testkit::contract::Capability;
use mivia_testkit::contract::clock::ClockHarness;
use mivia_testkit::contract::fs::FsHarness;
use mivia_testkit::contract::watch::WatchHarness;
use mivia_testkit::watch::RecordingSink;
use mivia_testkit::{clock_contract_tests, fs_contract_tests, watch_contract_tests};
use parking_lot::Mutex;
use tempfile::TempDir;

const FILE_SHARE_READ: u32 = 1;

fn native(path: &WinPath) -> PathBuf {
    PathBuf::from(path.to_string())
}

fn write(path: &WinPath, bytes: &[u8]) {
    let target = native(path);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(target, bytes).unwrap();
}

fn root_of(dir: &TempDir) -> WinPath {
    let canonical = std::fs::canonicalize(dir.path()).unwrap();
    WinPath::parse(canonical.to_str().unwrap()).unwrap()
}

struct RealFs {
    dir: TempDir,
    held: Mutex<BTreeMap<String, File>>,
}

impl RealFs {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
            held: Mutex::new(BTreeMap::new()),
        }
    }
}

#[async_trait]
impl FsHarness for RealFs {
    fn sut(&self) -> Arc<dyn FileSystem> {
        Arc::new(OsFileSystem)
    }

    fn root(&self) -> WinPath {
        root_of(&self.dir)
    }

    fn capabilities(&self) -> &[Capability] {
        &[
            Capability::CaseInsensitive,
            Capability::ReparsePoints,
            Capability::ShareModes,
            Capability::LongPaths,
        ]
    }

    async fn put(&self, path: &WinPath, bytes: &[u8]) {
        write(path, bytes);
    }

    async fn lock(&self, path: &WinPath) {
        let writer = File::options()
            .write(true)
            .share_mode(FILE_SHARE_READ)
            .open(native(path))
            .unwrap();
        self.held.lock().insert(path.key(), writer);
    }

    async fn unlock(&self, path: &WinPath) {
        self.held.lock().remove(&path.key());
    }

    async fn link_dir(&self, link: &WinPath, target: &WinPath) {
        let status = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(native(link))
            .arg(native(target))
            .output()
            .unwrap();
        assert!(status.status.success(), "mklink failed");
    }
}

struct RealWatch {
    dir: TempDir,
    sink: Arc<RecordingSink>,
    watcher: Arc<NotifyWatcher>,
}

impl RealWatch {
    fn new() -> Self {
        let sink = Arc::new(RecordingSink::default());
        Self {
            dir: tempfile::tempdir().unwrap(),
            watcher: Arc::new(NotifyWatcher::new(Arc::clone(&sink) as _)),
            sink,
        }
    }
}

#[async_trait]
impl WatchHarness for RealWatch {
    fn sut(&self) -> Arc<dyn FolderWatcher> {
        Arc::clone(&self.watcher) as _
    }

    fn root(&self) -> WinPath {
        root_of(&self.dir)
    }

    fn signals(&self) -> Vec<FsSignal> {
        self.sink.take()
    }

    async fn put(&self, path: &WinPath, bytes: &[u8]) {
        write(path, bytes);
    }

    async fn remove(&self, path: &WinPath) {
        std::fs::remove_file(native(path)).unwrap();
    }

    async fn rename(&self, from: &WinPath, to: &WinPath) {
        std::fs::rename(native(from), native(to)).unwrap();
    }

    async fn pause(&self) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

struct RealClock;

#[async_trait]
impl ClockHarness for RealClock {
    fn sut(&self) -> Arc<dyn Clock> {
        Arc::new(SystemClock)
    }

    async fn advance(&self, ms: i64) {
        let wait = u64::try_from(ms).unwrap() + 5;
        tokio::time::sleep(Duration::from_millis(wait)).await;
    }
}

fs_contract_tests!(RealFs::new());
watch_contract_tests!(RealWatch::new());
clock_contract_tests!(RealClock);

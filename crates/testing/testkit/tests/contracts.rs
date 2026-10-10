use std::sync::Arc;

use async_trait::async_trait;
use mivia_domain::ids::Timestamp;
use mivia_domain::path::WinPath;
use mivia_ports::clock::Clock;
use mivia_ports::fs::FileSystem;
use mivia_ports::server::ServerApi;
use mivia_ports::watch::{FolderWatcher, FsSignal, SignalKind};
use mivia_testkit::clock::ManualClock;
use mivia_testkit::contract::Capability;
use mivia_testkit::contract::clock::ClockHarness;
use mivia_testkit::contract::fs::FsHarness;
use mivia_testkit::contract::server::ServerHarness;
use mivia_testkit::contract::watch::WatchHarness;
use mivia_testkit::fs::InMemoryFs;
use mivia_testkit::server::{FakeServerApi, ServerSim};
use mivia_testkit::watch::{FakeFolderWatcher, RecordingSink};
use mivia_testkit::{
    clock_contract_tests, fs_contract_tests, server_contract_tests, watch_contract_tests,
};

struct ManualClockHarness(Arc<ManualClock>);

#[async_trait]
impl ClockHarness for ManualClockHarness {
    fn sut(&self) -> Arc<dyn Clock> {
        Arc::clone(&self.0) as _
    }

    async fn advance(&self, ms: i64) {
        self.0.advance(ms);
    }
}

struct MemoryFsHarness(Arc<InMemoryFs>);

impl MemoryFsHarness {
    fn new() -> Self {
        let fs = InMemoryFs::windows();
        fs.mkdir(&WinPath::parse(r"D:\Lab").unwrap());
        Self(Arc::new(fs))
    }
}

#[async_trait]
impl FsHarness for MemoryFsHarness {
    fn sut(&self) -> Arc<dyn FileSystem> {
        Arc::clone(&self.0) as _
    }

    fn root(&self) -> WinPath {
        WinPath::parse(r"D:\Lab").unwrap()
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
        self.0.put(path, bytes);
    }

    async fn lock(&self, path: &WinPath) {
        self.0.lock(path);
    }

    async fn unlock(&self, path: &WinPath) {
        self.0.unlock(path);
    }

    async fn link_dir(&self, link: &WinPath, _target: &WinPath) {
        self.0.link_dir(link);
    }
}

struct FakeWatchHarness {
    fs: Arc<InMemoryFs>,
    sink: Arc<RecordingSink>,
    watcher: Arc<FakeFolderWatcher>,
}

impl FakeWatchHarness {
    fn new() -> Self {
        let fs = Arc::new(InMemoryFs::windows());
        fs.mkdir(&WinPath::parse(r"D:\Lab").unwrap());
        let sink = Arc::new(RecordingSink::default());
        let watcher = Arc::new(FakeFolderWatcher::new(
            Arc::clone(&sink) as _,
            Arc::clone(&fs),
        ));
        Self { fs, sink, watcher }
    }
}

#[async_trait]
impl WatchHarness for FakeWatchHarness {
    fn sut(&self) -> Arc<dyn FolderWatcher> {
        Arc::clone(&self.watcher) as _
    }

    fn root(&self) -> WinPath {
        WinPath::parse(r"D:\Lab").unwrap()
    }

    fn signals(&self) -> Vec<FsSignal> {
        self.sink.take()
    }

    async fn put(&self, path: &WinPath, bytes: &[u8]) {
        self.fs.put(path, bytes);
        self.watcher.emit(SignalKind::Touched(path.clone()));
    }

    async fn remove(&self, path: &WinPath) {
        self.fs.vanish(path);
        self.watcher.emit(SignalKind::Removed(path.clone()));
    }

    async fn rename(&self, from: &WinPath, to: &WinPath) {
        self.fs.vanish(from);
        self.fs.put(to, b"r");
        self.watcher.emit(SignalKind::RenamedFrom(from.clone()));
        self.watcher.emit(SignalKind::RenamedTo(to.clone()));
    }

    async fn pause(&self) {}
}

struct FakeServerHarness(Arc<FakeServerApi>);

#[async_trait]
impl ServerHarness for FakeServerHarness {
    fn sut(&self) -> Arc<dyn ServerApi> {
        Arc::clone(&self.0) as _
    }

    async fn finish_jobs(&self) {
        self.0.with_sim(ServerSim::finish_all);
    }
}

clock_contract_tests!(ManualClockHarness(Arc::new(ManualClock::new(Timestamp(
    1_000
)))));
fs_contract_tests!(MemoryFsHarness::new());
server_contract_tests!(FakeServerHarness(Arc::new(FakeServerApi::default())));
watch_contract_tests!(FakeWatchHarness::new());

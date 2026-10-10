use std::sync::Arc;

use async_trait::async_trait;
use mivia_domain::ids::Timestamp;
use mivia_domain::path::WinPath;
use mivia_ports::clock::Clock;
use mivia_ports::fs::FileSystem;
use mivia_ports::server::ServerApi;
use mivia_testkit::clock::ManualClock;
use mivia_testkit::contract::Capability;
use mivia_testkit::contract::clock::ClockHarness;
use mivia_testkit::contract::fs::FsHarness;
use mivia_testkit::contract::server::ServerHarness;
use mivia_testkit::fs::InMemoryFs;
use mivia_testkit::server::{FakeServerApi, ServerSim};
use mivia_testkit::{clock_contract_tests, fs_contract_tests, server_contract_tests};

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

use std::future::Future;
use std::sync::Arc;

use mivia_application::pipeline::{Pipeline, PipelinePorts, Tick};
use mivia_application::report_settings::ReportSettings;
use mivia_domain::hold::HoldReason;
use mivia_domain::ids::{MINUTE_MS, SECOND_MS, Timestamp};
use mivia_ports::clock::Clock;
use mivia_ports::store::StoreExt;
use mivia_testkit::auth::FakeAuthorizer;
use mivia_testkit::clock::ManualClock;
use mivia_testkit::id_gen::SeededIdGen;
use mivia_testkit::server::{FakeServerApi, Fault, Op};
use mivia_testkit::telemetry::RecordingTelemetry;

use super::world::{World, jpeg, win};

const ROUNDS: usize = 400;
pub(crate) const STAGING: &str = r"C:\App\reports\staging";
const CRASH_POLLS: usize = 100_000;

pub(crate) async fn with_clock<F: Future>(clock: &ManualClock, work: F) -> F::Output {
    let mut work = std::pin::pin!(work);
    loop {
        tokio::select! {
            biased;
            out = &mut work => return out,
            () = tokio::task::yield_now() => {
                if let Some(at) = clock.pending_waiters().into_iter().min() {
                    clock.set(at.max(clock.now()));
                }
            }
        }
    }
}

pub(crate) struct Line {
    pub(crate) world: World,
    pub(crate) server: Arc<FakeServerApi>,
    pub(crate) auth: Arc<FakeAuthorizer>,
    pub(crate) pipeline: Pipeline,
    pub(crate) telemetry: Arc<RecordingTelemetry>,
}

impl Line {
    pub(crate) async fn new() -> Self {
        Self::with_reports(ReportSettings::default()).await
    }

    pub(crate) async fn with_reports(settings: ReportSettings) -> Self {
        let world = World::new().await;
        world.add_with(MINUTE_MS, false, settings).await;
        let server = Arc::new(FakeServerApi::new(Arc::clone(&world.clock) as _));
        let auth = Arc::new(FakeAuthorizer::new(world.scope.0));
        let telemetry = Arc::new(RecordingTelemetry::default());
        let pipeline = pipeline(&world, (&server, &auth, &telemetry), 1);
        pipeline.start().await.unwrap();
        Self {
            world,
            server,
            auth,
            pipeline,
            telemetry,
        }
    }

    pub(crate) async fn restart(&mut self, seed: u64) {
        let wiring = (&self.server, &self.auth, &self.telemetry);
        self.pipeline = pipeline(&self.world, wiring, seed);
        self.pipeline.start().await.unwrap();
    }

    pub(crate) async fn crash_during<F: Future>(&self, op: Op, work: F) {
        let before = self.server.count(op);
        self.server.fail_next(op, Fault::Hang);
        let mut work = std::pin::pin!(work);
        for _ in 0..CRASH_POLLS {
            if self.server.count(op) > before {
                return;
            }
            tokio::select! {
                biased;
                _ = &mut work => panic!("the work finished before the crash point"),
                () = tokio::task::yield_now() => {}
            }
        }
        panic!("the crash point was never reached");
    }

    pub(crate) fn now(&self) -> Timestamp {
        self.world.clock.now()
    }

    pub(crate) async fn files(&self, count: u8) {
        for n in 0..count {
            self.world
                .drop_file(&format!(r"D:\Lab\image_{n:03}.jpg"), &jpeg(n));
        }
        self.world.pass(2 * SECOND_MS).await;
    }

    pub(crate) async fn tick(&self) -> [Tick; 7] {
        let pipeline = &self.pipeline;
        let clock = &self.world.clock;
        [
            with_clock(clock, pipeline.upload_tick()).await.unwrap(),
            with_clock(clock, pipeline.submit_tick()).await.unwrap(),
            with_clock(clock, pipeline.recovery_tick()).await.unwrap(),
            with_clock(clock, pipeline.poll_tick()).await.unwrap(),
            with_clock(clock, pipeline.probe_tick()).await.unwrap(),
            with_clock(clock, pipeline.report_tick()).await.unwrap(),
            with_clock(clock, pipeline.save_tick()).await.unwrap(),
        ]
    }

    pub(crate) async fn drive(&self) {
        for _ in 0..ROUNDS {
            self.world.settle().await;
            if !self.tick().await.contains(&Tick::Busy) {
                return;
            }
        }
        panic!("the pipeline did not settle");
    }

    pub(crate) async fn run_for(&self, ms: i64, step: i64) {
        let end = self.now().plus_ms(ms);
        while self.now() < end {
            self.drive().await;
            self.world.clock.advance(step);
            self.world.fs.set_now(self.now());
        }
        self.drive().await;
    }

    pub(crate) async fn states(&self) -> Vec<String> {
        let mut states = Vec::new();
        for series in self.world.series().await {
            for item in self.world.items(&series).await {
                states.extend(item.computations.into_iter().map(|c| c.state));
            }
        }
        states
    }

    pub(crate) async fn file_states(&self) -> Vec<String> {
        let mut states = Vec::new();
        for series in self.world.series().await {
            let items = self.world.items(&series).await;
            states.extend(items.into_iter().map(|item| item.state));
        }
        states
    }

    pub(crate) async fn holds(&self) -> Vec<HoldReason> {
        let scope = self.world.scope;
        let store = &self.world.store;
        store
            .write(move |tx| tx.holds(scope).active())
            .await
            .unwrap()
    }

    pub(crate) async fn upload_attempts(&self) -> Vec<u32> {
        let scope = self.world.scope;
        let far = self.now().plus_ms(24 * 60 * MINUTE_MS);
        let rows = self
            .world
            .store
            .write(move |tx| tx.pipeline(scope).upload_canary(far))
            .await
            .unwrap();
        rows.into_iter().map(|row| row.attempts).collect()
    }

    pub(crate) async fn finish(self) {
        self.world.finish().await;
    }
}

fn pipeline(
    world: &World,
    (server, auth, telemetry): (
        &Arc<FakeServerApi>,
        &Arc<FakeAuthorizer>,
        &Arc<RecordingTelemetry>,
    ),
    seed: u64,
) -> Pipeline {
    let ports = PipelinePorts {
        store: Arc::clone(&world.store) as _,
        fs: Arc::clone(&world.fs) as _,
        server: Arc::clone(server) as _,
        authorizer: Arc::clone(auth) as _,
        clock: Arc::clone(&world.clock) as _,
        ids: Arc::new(SeededIdGen::new(seed)),
        telemetry: Arc::clone(telemetry) as _,
        staging: win(STAGING),
    };
    Pipeline::new(ports, world.scope, 2)
}

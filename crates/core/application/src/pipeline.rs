mod catalogue;
mod effects;
mod fair;
mod holds;
mod outcome;
mod poll;
mod probe;
mod recovery;
mod report;
mod report_poll;
mod report_save;
mod submit;
mod upload;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use mivia_domain::hold::{Access, Gate, HoldReason};
use mivia_domain::ids::{ComputationId, MINUTE_MS, SECOND_MS, SourceFileId, Timestamp};
use mivia_domain::path::WinPath;
use mivia_domain::report::REPORTS_V2_MIN_SERVER_VERSION;
use mivia_domain::retry::RetryPolicy;
use mivia_ports::auth::Authorizer;
use mivia_ports::clock::Clock;
use mivia_ports::fs::FileSystem;
use mivia_ports::id_gen::IdGen;
use mivia_ports::server::ServerApi;
use mivia_ports::store::pipeline::Clamp;
use mivia_ports::store::{AccountScope, Store, StoreError, StoreExt, TxError, WriteTx};
use mivia_ports::telemetry::TelemetrySink;
use parking_lot::Mutex;
use tokio::sync::Notify;

use crate::network::Network;

pub use crate::worker::{Tick, run};
pub use report_save::SavedCheck;

const MAX_POLL_GAP_MS: i64 = 5 * MINUTE_MS;
const MAX_PROBE_GAP_MS: i64 = 30 * MINUTE_MS;
const RESUME_SPREAD: RetryPolicy = RetryPolicy::probe(30 * SECOND_MS, 30 * SECOND_MS);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PipelineError {
    Store(StoreError),
}

impl From<StoreError> for PipelineError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

pub struct PipelinePorts {
    pub store: Arc<dyn Store>,
    pub fs: Arc<dyn FileSystem>,
    pub server: Arc<dyn ServerApi>,
    pub authorizer: Arc<dyn Authorizer>,
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<dyn IdGen>,
    pub telemetry: Arc<dyn TelemetrySink>,
    pub network: Arc<Network>,
    pub staging: WinPath,
}

#[derive(Default)]
pub struct PipelineWakers {
    pub uploader: Notify,
    pub submitter: Notify,
    pub poller: Notify,
    pub recovery: Notify,
    pub probe: Notify,
    pub reports: Notify,
    pub saver: Notify,
}

impl PipelineWakers {
    pub fn all(&self) {
        for notify in [
            &self.uploader,
            &self.submitter,
            &self.poller,
            &self.recovery,
            &self.probe,
            &self.reports,
            &self.saver,
        ] {
            notify.notify_one();
        }
    }
}

#[derive(Default)]
struct InFlight {
    uploads: BTreeSet<SourceFileId>,
    submits: BTreeSet<ComputationId>,
    recovery: BTreeSet<String>,
}

struct ReportState {
    v2_min: Mutex<Option<String>>,
    server_version: Mutex<Option<(String, Timestamp)>>,
    paused_until: Mutex<Option<Timestamp>>,
}

impl Default for ReportState {
    fn default() -> Self {
        Self {
            v2_min: Mutex::new(REPORTS_V2_MIN_SERVER_VERSION.map(str::to_owned)),
            server_version: Mutex::new(None),
            paused_until: Mutex::new(None),
        }
    }
}

#[derive(Default)]
struct Schedule {
    due: BTreeMap<&'static str, Timestamp>,
    last_status_request: Option<Timestamp>,
    omitted: BTreeMap<ComputationId, Timestamp>,
}

pub struct Pipeline {
    ports: PipelinePorts,
    scope: AccountScope,
    upload_permits: usize,
    in_flight: Mutex<InFlight>,
    schedule: Mutex<Schedule>,
    holds: holds::HoldState,
    reports: ReportState,
    pub wakers: Arc<PipelineWakers>,
}

impl Pipeline {
    pub fn new(ports: PipelinePorts, scope: AccountScope, upload_permits: usize) -> Self {
        Self {
            ports,
            scope,
            upload_permits: upload_permits.clamp(1, 4),
            in_flight: Mutex::new(InFlight::default()),
            schedule: Mutex::new(Schedule::default()),
            holds: holds::HoldState::default(),
            reports: ReportState::default(),
            wakers: Arc::new(PipelineWakers::default()),
        }
    }

    pub async fn start(&self) -> Result<(), PipelineError> {
        let now = self.now();
        self.write(move |tx, scope| tx.pipeline(scope).recover_after_restart(now))
            .await?;
        self.clock_jumped().await
    }

    pub async fn clock_jumped(&self) -> Result<(), PipelineError> {
        let now = self.now();
        let clamp = Clamp {
            retry_until: now.plus_ms(RetryPolicy::QUEUE.cap_ms),
            poll_until: now.plus_ms(MAX_POLL_GAP_MS),
            probe_until: now.plus_ms(MAX_PROBE_GAP_MS),
        };
        self.write(move |tx, scope| {
            tx.pipeline(scope).clamp_schedule(clamp)?;
            tx.reports(scope).recover_parts(now)
        })
        .await?;
        let mut schedule = self.schedule.lock();
        schedule.due.clear();
        schedule.last_status_request = None;
        drop(schedule);
        self.wakers.all();
        Ok(())
    }

    pub async fn resumed(&self) -> Result<(), PipelineError> {
        self.clock_jumped().await?;
        let now = self.now();
        let network =
            self.active_holds().await?.into_iter().filter(|reason| {
                matches!(reason, HoldReason::Offline | HoldReason::NetworkBlocked(_))
            });
        let probes: Vec<(HoldReason, Timestamp)> = network
            .map(|reason| {
                (
                    reason,
                    now.plus_ms(RESUME_SPREAD.delay_ms(1, self.random())),
                )
            })
            .collect();
        self.write(move |tx, scope| {
            probes
                .iter()
                .try_for_each(|(reason, at)| tx.holds(scope).raise(*reason, now, Some(*at)))
        })
        .await?;
        self.wakers.probe.notify_one();
        Ok(())
    }

    pub async fn probe_now(&self) -> Result<(), PipelineError> {
        self.probe_within(0).await
    }

    pub async fn probe_within(&self, spread_ms: u64) -> Result<(), PipelineError> {
        let now = self.now();
        let delay = i64::try_from(self.random() % spread_ms.max(1)).unwrap_or_default();
        let at = now.plus_ms(delay);
        let held = self.active_holds().await?;
        self.write(move |tx, scope| {
            held.iter()
                .try_for_each(|reason| tx.holds(scope).raise(*reason, now, Some(at)))
        })
        .await?;
        self.wakers.probe.notify_one();
        Ok(())
    }

    pub async fn client_too_old(&self, held: bool) -> Result<(), PipelineError> {
        let active = self
            .active_holds()
            .await?
            .contains(&HoldReason::ClientTooOld);
        match (held, active) {
            (true, false) => self.raise_hold(HoldReason::ClientTooOld).await,
            (false, true) => self.lift_hold(HoldReason::ClientTooOld).await,
            (true, true) | (false, false) => Ok(()),
        }
    }

    pub fn busy(&self) -> bool {
        let in_flight = self.in_flight.lock();
        !in_flight.uploads.is_empty() || !in_flight.submits.is_empty()
    }

    pub async fn working(&self) -> Result<bool, PipelineError> {
        if self.busy() {
            return Ok(true);
        }
        let computing = self
            .write(|tx, scope| tx.pipeline(scope).computing_count())
            .await?;
        Ok(computing > 0)
    }

    fn now(&self) -> Timestamp {
        self.ports.clock.now()
    }

    fn random(&self) -> u64 {
        self.ports.ids.uuid().as_u64_pair().1
    }

    async fn write<R, F>(&self, work: F) -> Result<R, PipelineError>
    where
        R: Send + 'static,
        F: FnOnce(&mut dyn WriteTx, AccountScope) -> Result<R, TxError> + Send + 'static,
    {
        let scope = self.scope;
        Ok(self.ports.store.write(move |tx| work(tx, scope)).await?)
    }

    async fn access(&self, gate: Gate) -> Result<Access, PipelineError> {
        self.write(move |tx, scope| tx.holds(scope).access(gate))
            .await
    }

    async fn active_holds(&self) -> Result<Vec<HoldReason>, PipelineError> {
        self.write(|tx, scope| tx.holds(scope).active()).await
    }

    fn due_later(&self, worker: &'static str, at: Timestamp) {
        let mut schedule = self.schedule.lock();
        let due = schedule.due.entry(worker).or_insert(at);
        *due = (*due).min(at);
    }

    fn next_due(&self, worker: &'static str) -> Tick {
        let now = self.now();
        let mut schedule = self.schedule.lock();
        match schedule.due.get(worker).copied() {
            Some(at) if at > now => Tick::DueAt(at),
            Some(_) => {
                schedule.due.remove(worker);
                Tick::Busy
            }
            None => Tick::Idle,
        }
    }
}

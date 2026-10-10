mod sim;

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use async_trait::async_trait;
use mivia_domain::ids::Timestamp;
use mivia_ports::clock::Clock;
use mivia_ports::server::{
    CatalogueModel, FindJobs, ImageRef, LabelsFetch, License, ServerApi, ServerError, ServerJob,
    ServerVersion, SubmitRequest, UploadRequest,
};
use parking_lot::Mutex;

pub use sim::{Licence, ServerSim, SimJob, sim_id};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Op {
    Version,
    License,
    Catalogue,
    Labels,
    Upload,
    Submit,
    Status,
    Find,
    Exists,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fault {
    Before(ServerError),
    After(ServerError),
    PartialSubmit { created: usize },
    Omit,
    Hang,
}

#[derive(Default)]
struct Faults {
    queued: BTreeMap<Op, VecDeque<Fault>>,
    down: BTreeMap<Op, ServerError>,
}

#[derive(Default)]
pub struct FakeServerApi {
    sim: Mutex<ServerSim>,
    faults: Mutex<Faults>,
    journal: Mutex<Vec<(Op, Timestamp)>>,
    clock: Option<Arc<dyn Clock>>,
    skew_ms: AtomicI64,
}

impl FakeServerApi {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            clock: Some(clock),
            ..Self::default()
        }
    }

    pub fn with_sim<R>(&self, work: impl FnOnce(&mut ServerSim) -> R) -> R {
        work(&mut self.sim.lock())
    }

    pub fn fail_next(&self, op: Op, fault: Fault) {
        self.faults
            .lock()
            .queued
            .entry(op)
            .or_default()
            .push_back(fault);
    }

    pub fn set_down(&self, ops: &[Op], error: Option<&ServerError>) {
        let mut faults = self.faults.lock();
        for op in ops {
            match error {
                Some(error) => faults.down.insert(*op, error.clone()),
                None => faults.down.remove(op),
            };
        }
    }

    pub fn set_skew_ms(&self, skew: i64) {
        self.skew_ms.store(skew, Ordering::Relaxed);
    }

    pub fn journal(&self) -> Vec<(Op, Timestamp)> {
        self.journal.lock().clone()
    }

    pub fn count(&self, op: Op) -> usize {
        self.journal
            .lock()
            .iter()
            .filter(|(seen, _)| *seen == op)
            .count()
    }

    fn server_now(&self) -> Timestamp {
        let local = self
            .clock
            .as_ref()
            .map_or(Timestamp(0), |clock| clock.now());
        local.plus_ms(self.skew_ms.load(Ordering::Relaxed))
    }

    fn enter(&self, op: Op) -> Result<Option<Fault>, ServerError> {
        let at = self
            .clock
            .as_ref()
            .map_or(Timestamp(0), |clock| clock.now());
        self.journal.lock().push((op, at));
        let mut faults = self.faults.lock();
        if let Some(error) = faults.down.get(&op) {
            return Err(error.clone());
        }
        match faults.queued.get_mut(&op).and_then(VecDeque::pop_front) {
            Some(Fault::Before(error)) => Err(error),
            other => Ok(other),
        }
    }

    async fn after<T>(fault: Option<Fault>, value: T) -> Result<T, ServerError> {
        match fault {
            Some(Fault::After(error)) => Err(error),
            Some(Fault::Hang) => std::future::pending().await,
            Some(Fault::Before(_) | Fault::PartialSubmit { .. } | Fault::Omit) | None => Ok(value),
        }
    }

    fn partial(&self, request: &SubmitRequest, created: usize) -> Result<(), ServerError> {
        let now = self.server_now();
        let mut sim = self.sim.lock();
        sim.check_submit(request)?;
        let ids = sim.create_jobs(request, created, now);
        if let Some(last) = ids.last() {
            sim.fail_job(last);
        }
        Err(ServerError::Unavailable { status: 500 })
    }
}

async fn drain(mut request: UploadRequest) -> Result<Vec<u8>, ServerError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = request
        .source
        .next_chunk()
        .await
        .map_err(ServerError::Source)?
    {
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[async_trait]
impl ServerApi for FakeServerApi {
    async fn version(&self) -> Result<ServerVersion, ServerError> {
        let fault = self.enter(Op::Version)?;
        let version = self.sim.lock().version.clone();
        Self::after(fault, version).await
    }

    async fn license(&self) -> Result<License, ServerError> {
        let fault = self.enter(Op::License)?;
        let license = self.sim.lock().license.clone();
        Self::after(fault, license).await
    }

    async fn catalogue(&self) -> Result<Vec<CatalogueModel>, ServerError> {
        let fault = self.enter(Op::Catalogue)?;
        let catalogue = self.sim.lock().catalogue.clone();
        Self::after(fault, catalogue).await
    }

    async fn model_labels(
        &self,
        lang: &str,
        etag: Option<String>,
    ) -> Result<LabelsFetch, ServerError> {
        let fault = self.enter(Op::Labels)?;
        let stored = self.sim.lock().labels.get(lang).cloned();
        let fetch = match stored {
            Some((_, current)) if etag.as_deref() == Some(current.as_str()) => {
                LabelsFetch::NotModified
            }
            Some((body, current)) => LabelsFetch::Fresh {
                body,
                etag: Some(current),
            },
            None => return Err(ServerError::Gone),
        };
        Self::after(fault, fetch).await
    }

    async fn upload(&self, request: UploadRequest) -> Result<ImageRef, ServerError> {
        let fault = self.enter(Op::Upload)?;
        let bytes = drain(request).await?;
        let image = self.sim.lock().upload(&bytes)?;
        Self::after(fault, image).await
    }

    async fn submit(&self, request: SubmitRequest) -> Result<Vec<ServerJob>, ServerError> {
        let fault = self.enter(Op::Submit)?;
        if let Some(Fault::PartialSubmit { created }) = fault {
            return self.partial(&request, created).map(|()| Vec::new());
        }
        let now = self.server_now();
        let jobs = self.sim.lock().submit(&request, now)?;
        Self::after(fault, jobs).await
    }

    async fn job_statuses(&self, job_ids: Vec<String>) -> Result<Vec<ServerJob>, ServerError> {
        let fault = self.enter(Op::Status)?;
        let mut found = self.sim.lock().statuses(&job_ids)?;
        if fault == Some(Fault::Omit) && !found.is_empty() {
            found.remove(0);
        }
        Self::after(fault, found).await
    }

    async fn find_jobs(&self, query: FindJobs) -> Result<Vec<ServerJob>, ServerError> {
        let fault = self.enter(Op::Find)?;
        let found = self.sim.lock().find(&query);
        Self::after(fault, found).await
    }

    async fn job_exists(&self, job_id: &str) -> Result<bool, ServerError> {
        let fault = self.enter(Op::Exists)?;
        let exists = self.sim.lock().job_exists(job_id);
        Self::after(fault, exists).await
    }

    fn server_offset_ms(&self) -> Option<i64> {
        Some(self.skew_ms.load(Ordering::Relaxed))
    }
}

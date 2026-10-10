use std::collections::{BTreeMap, VecDeque};

use async_trait::async_trait;
use mivia_ports::server::{
    ImageRef, JobStatus, MAX_STATUS_IDS, ServerApi, ServerError, ServerJobStatus, SubmitItem,
    SubmitOutcome, UploadRequest, Whoami,
};
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Whoami,
    Upload { file_name: String },
    Submit { count: usize },
    Statuses { count: usize },
}

#[derive(Clone, Debug)]
struct Job {
    polls_left: u32,
    result_id: String,
}

pub struct ServerSim {
    pub account: Whoami,
    pub max_upload_bytes: usize,
    pub polls_until_done: u32,
    images: BTreeMap<[u8; 32], String>,
    jobs: BTreeMap<String, Job>,
    keys: BTreeMap<Uuid, String>,
    journal: Vec<Request>,
}

impl Default for ServerSim {
    fn default() -> Self {
        Self {
            account: Whoami {
                account_id: "acc-1".to_owned(),
                display_name: "Lab".to_owned(),
            },
            max_upload_bytes: 300 * 1024 * 1024,
            polls_until_done: 2,
            images: BTreeMap::new(),
            jobs: BTreeMap::new(),
            keys: BTreeMap::new(),
            journal: Vec::new(),
        }
    }
}

impl ServerSim {
    pub fn journal(&self) -> &[Request] {
        &self.journal
    }

    pub fn job_count(&self) -> usize {
        self.jobs.len()
    }

    pub fn finish_all(&mut self) {
        self.jobs.values_mut().for_each(|job| job.polls_left = 0);
    }

    pub fn whoami(&mut self) -> Whoami {
        self.journal.push(Request::Whoami);
        self.account.clone()
    }

    pub fn upload(&mut self, request: &UploadRequest) -> Result<ImageRef, ServerError> {
        self.journal.push(Request::Upload {
            file_name: request.file_name.clone(),
        });
        if request.bytes.len() > self.max_upload_bytes {
            return Err(ServerError::TooLarge);
        }
        let content: [u8; 32] = Sha256::digest(&request.bytes).into();
        let fresh = format!("img-{}", self.images.len() + 1);
        let duplicate = self.images.contains_key(&content);
        let image_id = self.images.entry(content).or_insert(fresh).clone();
        Ok(ImageRef {
            image_id,
            duplicate,
        })
    }

    pub fn submit(&mut self, items: &[SubmitItem]) -> Result<Vec<SubmitOutcome>, ServerError> {
        self.journal.push(Request::Submit { count: items.len() });
        let known = |item: &SubmitItem| self.images.values().any(|id| *id == item.image_id);
        if !items.iter().all(known) {
            return Err(ServerError::NotFound);
        }
        Ok(items.iter().map(|item| self.job_for(item)).collect())
    }

    fn job_for(&mut self, item: &SubmitItem) -> SubmitOutcome {
        let next = format!("job-{}", self.jobs.len() + 1);
        let job_id = self
            .keys
            .entry(item.idempotency_key)
            .or_insert(next)
            .clone();
        let polls_left = self.polls_until_done;
        self.jobs.entry(job_id.clone()).or_insert_with(|| Job {
            polls_left,
            result_id: format!("res-{job_id}"),
        });
        SubmitOutcome {
            idempotency_key: item.idempotency_key,
            job_id,
        }
    }

    pub fn job_statuses(&mut self, job_ids: &[String]) -> Result<Vec<JobStatus>, ServerError> {
        self.journal.push(Request::Statuses {
            count: job_ids.len(),
        });
        if job_ids.len() > MAX_STATUS_IDS {
            return Err(ServerError::BadRequest {
                code: "TOO_MANY_IDS".to_owned(),
            });
        }
        Ok(job_ids
            .iter()
            .filter_map(|id| self.jobs.get_mut(id).map(|job| poll(id, job)))
            .collect())
    }
}

fn poll(job_id: &str, job: &mut Job) -> JobStatus {
    let done = job.polls_left == 0;
    job.polls_left = job.polls_left.saturating_sub(1);
    JobStatus {
        job_id: job_id.to_owned(),
        status: if done {
            ServerJobStatus::New
        } else {
            ServerJobStatus::Pending
        },
        result_id: done.then(|| job.result_id.clone()),
    }
}

#[derive(Default)]
pub struct FakeServerApi {
    sim: Mutex<ServerSim>,
    faults: Mutex<VecDeque<ServerError>>,
}

impl FakeServerApi {
    pub fn with_sim<R>(&self, work: impl FnOnce(&mut ServerSim) -> R) -> R {
        work(&mut self.sim.lock())
    }

    pub fn fail_next(&self, error: ServerError) {
        self.faults.lock().push_back(error);
    }

    fn fault(&self) -> Result<(), ServerError> {
        self.faults.lock().pop_front().map_or(Ok(()), Err)
    }
}

#[async_trait]
impl ServerApi for FakeServerApi {
    async fn whoami(&self) -> Result<Whoami, ServerError> {
        self.fault()?;
        Ok(self.sim.lock().whoami())
    }

    async fn upload(&self, request: UploadRequest) -> Result<ImageRef, ServerError> {
        self.fault()?;
        self.sim.lock().upload(&request)
    }

    async fn submit(&self, items: Vec<SubmitItem>) -> Result<Vec<SubmitOutcome>, ServerError> {
        self.fault()?;
        self.sim.lock().submit(&items)
    }

    async fn job_statuses(&self, job_ids: Vec<String>) -> Result<Vec<JobStatus>, ServerError> {
        self.fault()?;
        self.sim.lock().job_statuses(&job_ids)
    }
}

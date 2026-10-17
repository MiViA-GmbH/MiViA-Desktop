use std::collections::BTreeMap;

use mivia_domain::ids::{ModelChoice, Timestamp};
use mivia_ports::server::{
    CatalogueModel, FIND_PAGE_SIZE, FindJobs, ImageRef, License, MAX_STATUS_IDS, MAX_SUBMIT_IMAGES,
    ServerError, ServerJob, ServerJobStatus, ServerVersion, SubmitRequest,
};
use sha2::{Digest, Sha256};

use super::reports::{ReportFaults, SimReport};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimJob {
    pub id: String,
    pub image_id: String,
    pub choice: ModelChoice,
    pub status: ServerJobStatus,
    pub result_id: Option<String>,
    pub created_at: Timestamp,
    polls_left: u32,
}

impl SimJob {
    fn view(&self) -> ServerJob {
        ServerJob {
            job_id: self.id.clone(),
            image_id: Some(self.image_id.clone()),
            customization_id: self.choice.customization_id.clone(),
            status: self.status.clone(),
            result_id: self.result_id.clone(),
            has_result: self.result_id.is_some(),
            created_at: Some(self.created_at),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Licence {
    Valid,
    NoNewImages,
    Invalid,
}

pub struct ServerSim {
    pub version: ServerVersion,
    pub license: License,
    pub licence: Licence,
    pub honour_idempotency: bool,
    pub polls_until_done: u32,
    pub max_upload_bytes: usize,
    pub catalogue: Vec<CatalogueModel>,
    pub labels: BTreeMap<String, (Vec<u8>, String)>,
    pub results: BTreeMap<String, String>,
    pub result_images: BTreeMap<(String, String), Vec<u8>>,
    images: BTreeMap<[u8; 32], String>,
    jobs: Vec<SimJob>,
    keys: BTreeMap<Uuid, Vec<String>>,
    pub report_faults: ReportFaults,
    pub(super) reports: Vec<SimReport>,
    counter: u64,
}

impl Default for ServerSim {
    fn default() -> Self {
        Self {
            version: ServerVersion {
                version: "2.45.0".to_owned(),
                ..ServerVersion::default()
            },
            license: License {
                kind: "UNLIMITED".to_owned(),
                ..License::default()
            },
            licence: Licence::Valid,
            honour_idempotency: false,
            polls_until_done: 2,
            max_upload_bytes: 300 * 1024 * 1024,
            catalogue: vec![CatalogueModel {
                model_id: sim_id(9, 1),
                internal_name: "GRAIN".to_owned(),
                name_en: "Grain size".to_owned(),
                name_de: "Korngröße".to_owned(),
                description_en: String::new(),
                description_de: String::new(),
                customizations: Vec::new(),
            }],
            labels: BTreeMap::new(),
            results: BTreeMap::new(),
            result_images: BTreeMap::new(),
            images: BTreeMap::new(),
            jobs: Vec::new(),
            keys: BTreeMap::new(),
            report_faults: ReportFaults::default(),
            reports: Vec::new(),
            counter: 0,
        }
    }
}

pub fn sim_id(kind: u32, n: u64) -> String {
    format!("{kind:08x}-0000-4000-8000-{n:012x}")
}

impl ServerSim {
    pub(super) fn next_id(&mut self) -> u64 {
        self.counter += 1;
        self.counter
    }

    pub fn jobs(&self) -> &[SimJob] {
        &self.jobs
    }

    pub fn jobs_for(&self, image_id: &str, choice: &ModelChoice) -> usize {
        let same = |job: &&SimJob| job.image_id == image_id && job.choice == *choice;
        self.jobs.iter().filter(same).count()
    }

    pub fn image_count(&self) -> usize {
        self.images.len()
    }

    pub fn finish_all(&mut self) {
        self.jobs.iter_mut().for_each(|job| job.polls_left = 0);
    }

    pub fn forget_images(&mut self) {
        self.images.clear();
    }

    pub fn upload(&mut self, bytes: &[u8]) -> Result<ImageRef, ServerError> {
        if bytes.len() > self.max_upload_bytes {
            return Err(ServerError::TooLarge { by_proxy: false });
        }
        let content: [u8; 32] = Sha256::digest(bytes).into();
        if let Some(image_id) = self.images.get(&content) {
            let image_id = image_id.clone();
            return Ok(ImageRef {
                image_id,
                duplicate: true,
            });
        }
        if self.licence != Licence::Valid {
            return Err(ServerError::LicenseInvalid);
        }
        let image_id = sim_id(1, self.next_id());
        self.images.insert(content, image_id.clone());
        Ok(ImageRef {
            image_id,
            duplicate: false,
        })
    }

    pub fn check_submit(&self, request: &SubmitRequest) -> Result<(), ServerError> {
        if request.image_ids.is_empty() || request.image_ids.len() > MAX_SUBMIT_IMAGES {
            return Err(ServerError::BadRequest {
                code: "VALIDATION".to_owned(),
            });
        }
        let known = |id: &&String| self.images.values().any(|image| image == *id);
        if let Some(missing) = request.image_ids.iter().find(|id| !known(id)) {
            return Err(ServerError::NotFound {
                image_id: Some(missing.clone()),
            });
        }
        match self.licence {
            Licence::Valid | Licence::NoNewImages => Ok(()),
            Licence::Invalid => Err(ServerError::LicenseInvalid),
        }
    }

    pub fn submit(
        &mut self,
        request: &SubmitRequest,
        now: Timestamp,
    ) -> Result<Vec<ServerJob>, ServerError> {
        self.check_submit(request)?;
        if let Some(ids) = self
            .keys
            .get(&request.key)
            .filter(|_| self.honour_idempotency)
        {
            let ids = ids.clone();
            return Ok(self.views(&ids));
        }
        let created = self.create_jobs(request, request.image_ids.len(), now);
        self.keys.insert(request.key, created.clone());
        Ok(self.views(&created))
    }

    pub fn create_jobs(
        &mut self,
        request: &SubmitRequest,
        count: usize,
        now: Timestamp,
    ) -> Vec<String> {
        let images: Vec<String> = request.image_ids.iter().take(count).cloned().collect();
        images
            .into_iter()
            .map(|image_id| self.create_job(image_id, request, now))
            .collect()
    }

    fn create_job(&mut self, image_id: String, request: &SubmitRequest, now: Timestamp) -> String {
        let choice = &request.choice;
        let id = sim_id(2, self.next_id());
        let computed = self.jobs.iter().find(|job| {
            job.image_id == image_id && job.choice == *choice && job.status == ServerJobStatus::New
        });
        let (status, result_id) = match computed {
            Some(job) if !request.force => (ServerJobStatus::Cached, job.result_id.clone()),
            Some(_) | None => (ServerJobStatus::Pending, None),
        };
        self.jobs.push(SimJob {
            id: id.clone(),
            image_id,
            choice: choice.clone(),
            status,
            result_id,
            created_at: now,
            polls_left: self.polls_until_done,
        });
        id
    }

    pub fn fail_job(&mut self, job_id: &str) {
        if let Some(job) = self.jobs.iter_mut().find(|job| job.id == job_id) {
            job.status = ServerJobStatus::Failed;
        }
    }

    pub fn delete_job(&mut self, job_id: &str) {
        self.jobs.retain(|job| job.id != job_id);
    }

    fn views(&self, ids: &[String]) -> Vec<ServerJob> {
        ids.iter()
            .filter_map(|id| self.jobs.iter().find(|job| job.id == *id))
            .map(SimJob::view)
            .collect()
    }

    pub fn statuses(&mut self, job_ids: &[String]) -> Result<Vec<ServerJob>, ServerError> {
        if job_ids.len() > MAX_STATUS_IDS {
            return Err(ServerError::BadRequest {
                code: "TOO_MANY_IDS".to_owned(),
            });
        }
        let mut found = Vec::new();
        for job in self.jobs.iter_mut().filter(|job| job_ids.contains(&job.id)) {
            advance(job);
            found.push(job.view());
        }
        Ok(found)
    }

    pub fn find(&self, query: &FindJobs) -> Vec<ServerJob> {
        let mut matching: Vec<&SimJob> = self
            .jobs
            .iter()
            .filter(|job| job.choice.model_id == query.choice.model_id)
            .collect();
        matching.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)));
        let page = usize::try_from(FIND_PAGE_SIZE).unwrap_or(usize::MAX);
        let skip = usize::try_from(query.page.saturating_sub(1)).unwrap_or(usize::MAX) * page;
        matching
            .into_iter()
            .skip(skip)
            .take(page)
            .map(SimJob::view)
            .collect()
    }

    pub fn delete_image(&mut self, image_id: &str) {
        self.images.retain(|_, id| id != image_id);
    }

    pub fn job_exists(&self, job_id: &str) -> bool {
        self.jobs.iter().any(|job| job.id == job_id)
    }
}

fn advance(job: &mut SimJob) {
    if job.status != ServerJobStatus::Pending {
        return;
    }
    if job.polls_left == 0 {
        job.status = ServerJobStatus::New;
        job.result_id = Some(job.id.replacen("00000002", "00000003", 1));
    } else {
        job.polls_left -= 1;
    }
}

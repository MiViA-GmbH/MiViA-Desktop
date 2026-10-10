use std::collections::BTreeSet;

use mivia_domain::computation::ComputationEvent;
use mivia_domain::hold::{Access, Gate};
use mivia_domain::ids::{ComputationId, ModelChoice, Timestamp};
use mivia_domain::polling::recovery_bound;
use mivia_ports::server::{FIND_PAGE_SIZE, FindJobs, ServerError, ServerJob, ServerJobStatus};
use mivia_ports::store::pipeline::{AttemptRow, JobRecord};

use super::effects::{Call, Item};
use super::outcome::record;
use super::submit::{Attempt, Member};
use super::{Pipeline, PipelineError, Tick};

const MAX_PAGES: u32 = 10;

struct Window<'a> {
    choice: &'a ModelChoice,
    bound: Timestamp,
    jobs: &'a [ServerJob],
}

impl Window<'_> {
    fn candidates<'s>(&'s self, image: &'s str) -> impl Iterator<Item = &'s ServerJob> {
        self.jobs.iter().filter(move |job| {
            let same_image = job.image_id.as_deref() == Some(image);
            let same_choice = job.customization_id == self.choice.customization_id;
            let usable = job.status != ServerJobStatus::Failed && self.recent(job);
            same_image && same_choice && usable
        })
    }

    fn recent(&self, job: &ServerJob) -> bool {
        job.created_at.is_some_and(|at| at >= self.bound)
    }
}

struct Matched {
    adopted: Vec<(ComputationId, JobRecord)>,
    unmatched: Vec<Member>,
}

fn members(attempt: &AttemptRow) -> Vec<Member> {
    attempt
        .members
        .iter()
        .map(|(id, image_id)| Member {
            id: *id,
            image_id: image_id.clone(),
            attempts: attempt.attempts,
        })
        .collect()
}

fn match_members(
    attempt: &AttemptRow,
    window: &Window<'_>,
    mut used: BTreeSet<String>,
    now: Timestamp,
) -> Matched {
    let mut matched = Matched {
        adopted: Vec::new(),
        unmatched: Vec::new(),
    };
    for member in members(attempt) {
        let found = window
            .candidates(&member.image_id)
            .filter(|job| !used.contains(&job.job_id))
            .min_by(|a, b| {
                a.created_at
                    .cmp(&b.created_at)
                    .then(a.job_id.cmp(&b.job_id))
            });
        match found {
            Some(job) => {
                used.insert(job.job_id.clone());
                let adopted = record(job, attempt.started_at, now);
                matched.adopted.push((member.id, adopted));
            }
            None => matched.unmatched.push(member),
        }
    }
    matched
}

impl Pipeline {
    pub async fn recovery_tick(&self) -> Result<Tick, PipelineError> {
        if self.access(Gate::Submit).await? == Access::Blocked {
            return Ok(Tick::Idle);
        }
        let now = self.now();
        let exclude: Vec<String> = self.in_flight.lock().recovery.iter().cloned().collect();
        let attempts = self
            .write(move |tx, scope| tx.pipeline(scope).recovery_due(now, &exclude))
            .await?;
        let Some(attempt) = attempts.into_iter().next() else {
            return Ok(self.next_due("recovery"));
        };
        let key = attempt.key.clone();
        self.in_flight.lock().recovery.insert(key.clone());
        let recovered = self.recover(attempt).await;
        self.in_flight.lock().recovery.remove(&key);
        recovered?;
        Ok(Tick::Busy)
    }

    async fn recover(&self, attempt: AttemptRow) -> Result<(), PipelineError> {
        let Some(choice) = ModelChoice::from_key(&attempt.choice_key) else {
            return self
                .events(&members(&attempt), ComputationEvent::ContractError)
                .await;
        };
        let offset = attempt
            .server_offset_ms
            .or_else(|| self.ports.server.server_offset_ms())
            .unwrap_or_default();
        let bound = recovery_bound(attempt.started_at, offset);
        let jobs = match self.list_since(&choice, bound).await {
            Ok(jobs) => jobs,
            Err(error) => return self.recovery_failed(&error, &attempt).await,
        };
        let window = Window {
            choice: &choice,
            bound,
            jobs: &jobs,
        };
        let used = self.referenced(&attempt, &window).await?;
        let matched = match_members(&attempt, &window, used, self.now());
        self.adopt(matched.adopted).await?;
        self.resubmit(attempt, matched.unmatched).await
    }

    async fn referenced(
        &self,
        attempt: &AttemptRow,
        window: &Window<'_>,
    ) -> Result<BTreeSet<String>, PipelineError> {
        let candidates: Vec<String> = attempt
            .members
            .iter()
            .flat_map(|(_, image)| window.candidates(image))
            .map(|job| job.job_id.clone())
            .collect();
        self.write(move |tx, scope| {
            let repo = tx.pipeline(scope);
            let mut known = BTreeSet::new();
            for job in candidates {
                if repo.job_known(&job)? {
                    known.insert(job);
                }
            }
            Ok(known)
        })
        .await
    }

    async fn adopt(&self, adopted: Vec<(ComputationId, JobRecord)>) -> Result<(), PipelineError> {
        let now = self.now();
        tracing::info!(adopted = adopted.len(), "recovery.adopted");
        self.write(move |tx, scope| {
            let repo = tx.pipeline(scope);
            adopted
                .iter()
                .try_for_each(|(id, job)| repo.record_job(*id, job, now))
        })
        .await
    }

    async fn resubmit(
        &self,
        attempt: AttemptRow,
        unmatched: Vec<Member>,
    ) -> Result<(), PipelineError> {
        if unmatched.is_empty() {
            return Ok(());
        }
        let key = if unmatched.len() == attempt.members.len() {
            attempt.key
        } else {
            self.ports.ids.uuid().to_string()
        };
        tracing::info!(resubmit = unmatched.len(), "recovery.resubmit");
        let resubmit = Attempt {
            key,
            choice_key: attempt.choice_key,
            members: unmatched,
        };
        self.send_attempt(resubmit).await.map(drop)
    }

    async fn list_since(
        &self,
        choice: &ModelChoice,
        bound: Timestamp,
    ) -> Result<Vec<ServerJob>, ServerError> {
        let mut all = Vec::new();
        for page in 1..=MAX_PAGES {
            let query = FindJobs {
                choice: choice.clone(),
                page,
            };
            let jobs = self.ports.server.find_jobs(query).await?;
            let last_page = jobs.len() < usize::try_from(FIND_PAGE_SIZE).unwrap_or(usize::MAX);
            let older = jobs
                .iter()
                .any(|job| job.created_at.is_some_and(|at| at < bound));
            all.extend(jobs);
            if last_page || older {
                break;
            }
        }
        Ok(all)
    }

    async fn recovery_failed(
        &self,
        error: &ServerError,
        attempt: &AttemptRow,
    ) -> Result<(), PipelineError> {
        let name = format!("recovery:{}", attempt.key);
        let item = Item::new(name.clone(), attempt.attempts);
        let retry = match self.infrastructure(error, Call::Other, item).await? {
            Some(retry) => retry,
            None => self.transient(name, attempt.attempts).await?,
        };
        self.delay(&members(attempt), retry).await
    }
}

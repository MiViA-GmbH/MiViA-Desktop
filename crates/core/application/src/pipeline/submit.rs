use std::collections::BTreeSet;

use mivia_domain::computation::ComputationEvent;
use mivia_domain::ids::{ComputationId, ModelChoice, Timestamp};
use mivia_ports::server::{MAX_SUBMIT_IMAGES, ServerError, ServerJob, SubmitRequest};
use mivia_ports::store::pipeline::{JobRecord, NewAttempt, Retry, SubmitRow};
use uuid::Uuid;

use super::effects::{Call, Item};
use super::fair::gather;
use super::outcome::record;
use super::{Pipeline, PipelineError, Tick};

const SUBMITTER: &str = "submitter";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Member {
    pub(super) id: ComputationId,
    pub(super) image_id: String,
    pub(super) attempts: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Attempt {
    pub(super) key: String,
    pub(super) choice_key: String,
    pub(super) members: Vec<Member>,
}

fn first_group(rows: Vec<SubmitRow>) -> Option<(String, Vec<Member>)> {
    let choice = rows.first()?.choice_key.clone();
    let mut images = BTreeSet::new();
    let members = rows
        .into_iter()
        .filter(|row| row.choice_key == choice && images.insert(row.image_id.clone()))
        .take(MAX_SUBMIT_IMAGES)
        .map(|row| Member {
            id: row.id,
            image_id: row.image_id,
            attempts: row.attempts,
        })
        .collect();
    Some((choice, members))
}

impl Pipeline {
    pub async fn submit_tick(&self) -> Result<Tick, PipelineError> {
        if !self.in_flight.lock().submits.is_empty() {
            return Ok(Tick::Idle);
        }
        let now = self.now();
        let rows = self
            .write(move |tx, scope| {
                let repo = tx.pipeline(scope);
                gather(|skip| repo.submit_due(now, &[], skip))
            })
            .await?;
        let Some((choice_key, members)) = first_group(rows) else {
            return Ok(self.next_due(SUBMITTER));
        };
        self.submit_new(choice_key, members).await?;
        Ok(Tick::Busy)
    }

    pub(super) async fn submit_new(
        &self,
        choice_key: String,
        members: Vec<Member>,
    ) -> Result<bool, PipelineError> {
        let attempt = Attempt {
            key: self.ports.ids.uuid().to_string(),
            choice_key,
            members,
        };
        self.send_attempt(attempt).await
    }

    pub(super) async fn send_attempt(&self, attempt: Attempt) -> Result<bool, PipelineError> {
        let (Some(choice), Ok(key)) = (
            ModelChoice::from_key(&attempt.choice_key),
            Uuid::parse_str(&attempt.key),
        ) else {
            self.events(&attempt.members, ComputationEvent::ContractError)
                .await?;
            return Ok(false);
        };
        let ids: Vec<ComputationId> = attempt.members.iter().map(|member| member.id).collect();
        self.in_flight.lock().submits.extend(ids.iter().copied());
        let started = self.begin(&attempt).await;
        let request = SubmitRequest {
            key,
            choice,
            image_ids: attempt.members.iter().map(|m| m.image_id.clone()).collect(),
            force: false,
        };
        let sent = match started {
            Ok(now) => self.send(&attempt, request, now).await,
            Err(error) => Err(error),
        };
        let mut in_flight = self.in_flight.lock();
        in_flight.submits.retain(|id| !ids.contains(id));
        drop(in_flight);
        self.wakers.poller.notify_one();
        self.wakers.recovery.notify_one();
        sent
    }

    async fn begin(&self, attempt: &Attempt) -> Result<Timestamp, PipelineError> {
        let now = self.now();
        let row = NewAttempt {
            key: attempt.key.clone(),
            choice_key: attempt.choice_key.clone(),
            started_at: now,
            server_offset_ms: self.ports.server.server_offset_ms(),
            members: attempt
                .members
                .iter()
                .map(|member| (member.id, member.image_id.clone()))
                .collect(),
        };
        self.write(move |tx, scope| tx.pipeline(scope).begin_attempt(&row, now))
            .await?;
        Ok(now)
    }

    async fn send(
        &self,
        attempt: &Attempt,
        request: SubmitRequest,
        started: Timestamp,
    ) -> Result<bool, PipelineError> {
        match self.ports.server.submit(request).await {
            Ok(jobs) => {
                self.settle_jobs(&attempt.members, &jobs, started).await?;
                Ok(true)
            }
            Err(error) => {
                Box::pin(self.submit_failed(&error, attempt)).await?;
                Ok(false)
            }
        }
    }

    async fn settle_jobs(
        &self,
        members: &[Member],
        jobs: &[ServerJob],
        started: Timestamp,
    ) -> Result<(), PipelineError> {
        let now = self.now();
        let settled: Vec<(ComputationId, Option<JobRecord>)> = members
            .iter()
            .map(|member| {
                let mut matching = jobs
                    .iter()
                    .filter(|job| job.image_id.as_deref() == Some(member.image_id.as_str()));
                let job = matching.next().filter(|_| matching.next().is_none());
                (member.id, job.map(|job| record(job, started, now)))
            })
            .collect();
        self.write(move |tx, scope| {
            let repo = tx.pipeline(scope);
            for (id, job) in settled {
                match job {
                    Some(job) => repo.record_job(id, &job, now)?,
                    None => repo.computation_event(id, ComputationEvent::Ambiguous, now)?,
                }
            }
            Ok(())
        })
        .await
    }

    pub(super) async fn events(
        &self,
        members: &[Member],
        event: ComputationEvent,
    ) -> Result<(), PipelineError> {
        let now = self.now();
        let ids: Vec<ComputationId> = members.iter().map(|member| member.id).collect();
        self.write(move |tx, scope| {
            let repo = tx.pipeline(scope);
            ids.iter()
                .try_for_each(|id| repo.computation_event(*id, event, now))
        })
        .await
    }

    pub(super) async fn delay(
        &self,
        members: &[Member],
        retry: Retry,
    ) -> Result<(), PipelineError> {
        self.due_later("recovery", retry.at);
        let ids: Vec<ComputationId> = members.iter().map(|member| member.id).collect();
        self.write(move |tx, scope| {
            let repo = tx.pipeline(scope);
            ids.iter()
                .try_for_each(|id| repo.computation_retry(*id, retry))
        })
        .await
    }

    async fn requeue(&self, members: &[Member], retry: Option<Retry>) -> Result<(), PipelineError> {
        let now = self.now();
        let changes: Vec<(ComputationId, Retry)> = members
            .iter()
            .map(|member| {
                let retry = retry.unwrap_or(Retry {
                    at: now,
                    attempts: member.attempts,
                });
                (member.id, retry)
            })
            .collect();
        self.due_later(
            SUBMITTER,
            changes.iter().map(|(_, r)| r.at).min().unwrap_or(now),
        );
        self.write(move |tx, scope| {
            let repo = tx.pipeline(scope);
            for (id, retry) in changes {
                repo.computation_event(id, ComputationEvent::NotSent, now)?;
                repo.computation_retry(id, retry)?;
            }
            Ok(())
        })
        .await
    }

    async fn submit_failed(
        &self,
        error: &ServerError,
        attempt: &Attempt,
    ) -> Result<(), PipelineError> {
        use ServerError as E;
        let members = &attempt.members;
        let attempts = members.iter().map(|m| m.attempts).max().unwrap_or_default();
        let item = Item::new(format!("submit:{}", attempt.key), attempts);
        match error {
            E::AfterSend
            | E::StallTimeout
            | E::Unavailable { .. }
            | E::InProgress
            | E::Contract => {
                let retry = self.transient(item.name, item.attempts).await?;
                self.events(members, ComputationEvent::Ambiguous).await?;
                self.delay(members, retry).await
            }
            E::NotFound { image_id } => self.isolate(attempt, image_id.as_deref()).await,
            E::Forbidden { .. } => self.refused(attempt).await,
            E::BadRequest { .. } | E::ImageRejected { .. } | E::TooLarge { .. } | E::Source(_) => {
                self.events(members, ComputationEvent::ContractError).await
            }
            E::Unreachable
            | E::TlsUntrusted
            | E::ProxyAuth
            | E::ProxyRefused
            | E::Redirect
            | E::RateLimited { .. }
            | E::Unauthorized(_)
            | E::LicenseInvalid
            | E::Gone
            | E::NotSignedIn
            | E::AccountMismatch => {
                let retry = self.infrastructure(error, Call::Submit, item).await?;
                self.requeue(members, retry).await
            }
        }
    }

    async fn refused(&self, attempt: &Attempt) -> Result<(), PipelineError> {
        let available = self.refresh_catalogue().await?;
        let choice_gone = available.is_some_and(|keys| !keys.contains(&attempt.choice_key));
        self.events(&attempt.members, ComputationEvent::Refused { choice_gone })
            .await
    }

    async fn isolate(&self, attempt: &Attempt, named: Option<&str>) -> Result<(), PipelineError> {
        let gone = named.and_then(|image| attempt.members.iter().find(|m| m.image_id == image));
        let single = attempt
            .members
            .first()
            .filter(|_| attempt.members.len() == 1);
        if let Some(gone) = gone.or(single) {
            let image = gone.image_id.clone();
            let now = self.now();
            self.write(move |tx, scope| tx.pipeline(scope).image_gone(&image, now))
                .await?;
            return self.requeue(&attempt.members, None).await;
        }
        let half = attempt.members.len() / 2;
        let (left, right) = attempt.members.split_at(half);
        for part in [left, right] {
            Box::pin(self.submit_new(attempt.choice_key.clone(), part.to_vec())).await?;
        }
        Ok(())
    }
}

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use mivia_domain::ids::ModelChoice;
use mivia_ports::fs::{ChunkSource, FsError};
use mivia_ports::server::{
    FindJobs, MAX_STATUS_IDS, ServerApi, ServerError, ServerJob, ServerJobStatus, SubmitRequest,
    UploadRequest,
};
use uuid::Uuid;

use super::{CaseResult, check, check_eq};
use crate::server::sim_id;

#[async_trait]
pub trait ServerHarness: Send + Sync {
    fn sut(&self) -> Arc<dyn ServerApi>;
    async fn finish_jobs(&self);
    async fn seed_results(&self, job_id: &str, results: &str, image: (&str, &[u8]));
}

pub struct BytesSource {
    chunks: Vec<Result<Vec<u8>, FsError>>,
}

#[async_trait]
impl ChunkSource for BytesSource {
    async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, FsError> {
        if self.chunks.is_empty() {
            return Ok(None);
        }
        self.chunks.remove(0).map(Some)
    }
}

pub fn upload_request(file_name: &str, bytes: &[u8]) -> UploadRequest {
    UploadRequest {
        file_name: file_name.to_owned(),
        len: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        source: Box::new(BytesSource {
            chunks: vec![Ok(bytes.to_vec())],
        }),
    }
}

static KEYS: AtomicU64 = AtomicU64::new(1);

fn failed(error: &ServerError) -> String {
    format!("{error:?}")
}

pub fn grain() -> ModelChoice {
    ModelChoice {
        model_id: sim_id(9, 1),
        customization_id: None,
    }
}

pub(super) async fn image(server: &dyn ServerApi, bytes: &[u8]) -> Result<String, String> {
    let uploaded = server.upload(upload_request("a.png", bytes)).await;
    Ok(uploaded.map_err(|e| failed(&e))?.image_id)
}

pub(super) async fn submit(
    server: &dyn ServerApi,
    images: &[&str],
) -> Result<Vec<ServerJob>, String> {
    let request = SubmitRequest {
        key: Uuid::from_u64_pair(0, KEYS.fetch_add(1, Ordering::Relaxed)),
        choice: grain(),
        image_ids: images.iter().map(|id| (*id).to_owned()).collect(),
        force: false,
    };
    server.submit(request).await.map_err(|e| failed(&e))
}

pub async fn server_same_content_is_one_image(harness: &impl ServerHarness) -> CaseResult {
    let server = harness.sut();
    let first = image(server.as_ref(), b"same").await?;
    let again = server
        .upload(upload_request("b.png", b"same"))
        .await
        .map_err(|e| failed(&e))?;
    check_eq(
        &(again.image_id, again.duplicate),
        &(first, true),
        "second upload",
    )
}

pub async fn server_broken_source_is_not_uploaded(harness: &impl ServerHarness) -> CaseResult {
    let request = UploadRequest {
        file_name: "c.png".to_owned(),
        len: 8,
        source: Box::new(BytesSource {
            chunks: vec![Ok(b"half".to_vec()), Err(FsError::Changed)],
        }),
    };
    let outcome = harness.sut().upload(request).await;
    check_eq(
        &outcome.map(|image| image.image_id),
        &Err(ServerError::Source(FsError::Changed)),
        "changed source",
    )
}

pub async fn server_jobs_map_to_their_images(harness: &impl ServerHarness) -> CaseResult {
    let server = harness.sut();
    let first = image(server.as_ref(), b"first").await?;
    let second = image(server.as_ref(), b"second").await?;
    let jobs = submit(server.as_ref(), &[&first, &second]).await?;
    let images: Vec<Option<String>> = jobs.iter().map(|job| job.image_id.clone()).collect();
    check_eq(&images, &vec![Some(first), Some(second)], "job images")?;
    check(
        jobs.iter()
            .all(|job| job.status == ServerJobStatus::Pending),
        "new jobs are not pending",
    )
}

pub async fn server_status_accepts_at_most_100_ids(harness: &impl ServerHarness) -> CaseResult {
    let ids = (0..=MAX_STATUS_IDS)
        .map(|n| sim_id(2, 900 + u64::try_from(n).unwrap_or_default()))
        .collect();
    let refused = harness.sut().job_statuses(ids).await;
    check(refused.is_err(), "101 status ids were accepted")?;
    let unknown = harness.sut().job_statuses(vec![sim_id(2, 999)]).await;
    check_eq(&unknown, &Ok(Vec::new()), "unknown id")
}

pub async fn server_jobs_finish_with_a_result(harness: &impl ServerHarness) -> CaseResult {
    let server = harness.sut();
    let done = image(server.as_ref(), b"done").await?;
    let jobs = submit(server.as_ref(), &[&done]).await?;
    let ids: Vec<String> = jobs.into_iter().map(|job| job.job_id).collect();
    harness.finish_jobs().await;
    let statuses = server
        .job_statuses(ids.clone())
        .await
        .map_err(|e| failed(&e))?;
    let finished = statuses
        .iter()
        .all(|job| job.status == ServerJobStatus::New && job.result_id.is_some());
    check(
        finished && statuses.len() == 1,
        "finished job has no result",
    )?;
    let again = submit(server.as_ref(), &[&done]).await?;
    let cached = again
        .first()
        .map(|job| (job.status.clone(), job.has_result));
    check_eq(
        &cached,
        &Some((ServerJobStatus::Cached, true)),
        "cached job",
    )?;
    let first = ids.first().cloned().unwrap_or_default();
    let exists = server.job_exists(&first).await.map_err(|e| failed(&e))?;
    check(exists, "finished job does not exist")
}

pub async fn server_lists_jobs_newest_first(harness: &impl ServerHarness) -> CaseResult {
    let server = harness.sut();
    let listed = image(server.as_ref(), b"listed").await?;
    let jobs = submit(server.as_ref(), &[&listed]).await?;
    let found = server
        .find_jobs(FindJobs {
            choice: grain(),
            page: 1,
        })
        .await
        .map_err(|e| failed(&e))?;
    let first = found.first().map(|job| job.job_id.clone());
    check_eq(
        &first,
        &jobs.first().map(|job| job.job_id.clone()),
        "newest job",
    )?;
    let gone = server
        .job_exists(&sim_id(2, 999))
        .await
        .map_err(|e| failed(&e))?;
    check(!gone, "unknown job exists")
}

pub async fn server_deletes_a_job_once_and_then_finds_it_gone(
    harness: &impl ServerHarness,
) -> CaseResult {
    let server = harness.sut();
    let deleted = image(server.as_ref(), b"deleted").await?;
    let jobs = submit(server.as_ref(), &[&deleted]).await?;
    let job = jobs
        .first()
        .map(|job| job.job_id.clone())
        .unwrap_or_default();
    server.delete_job(&job).await.map_err(|e| failed(&e))?;
    let exists = server.job_exists(&job).await.map_err(|e| failed(&e))?;
    check(!exists, "deleted job still exists")?;
    let again = server.delete_job(&job).await;
    check_eq(&again, &Ok(()), "deleting a gone job")?;
    server
        .delete_image(&deleted)
        .await
        .map_err(|e| failed(&e))?;
    let twice = server.delete_image(&deleted).await;
    check_eq(&twice, &Ok(()), "deleting a gone image")
}

pub async fn server_unknown_image_is_named(harness: &impl ServerHarness) -> CaseResult {
    let server = harness.sut();
    let known = image(server.as_ref(), b"known").await?;
    let missing = sim_id(1, 999);
    let outcome = submit(server.as_ref(), &[&known, &missing]).await;
    check_eq(
        &outcome.map(|jobs| jobs.len()),
        &Err(failed(&ServerError::NotFound {
            image_id: Some(missing),
        })),
        "missing image",
    )
}

#[macro_export]
macro_rules! server_contract_tests {
    ($harness:expr) => {
        $crate::contract_cases!(
            server,
            $harness,
            server_same_content_is_one_image,
            server_broken_source_is_not_uploaded,
            server_jobs_map_to_their_images,
            server_status_accepts_at_most_100_ids,
            server_jobs_finish_with_a_result,
            server_lists_jobs_newest_first,
            server_deletes_a_job_once_and_then_finds_it_gone,
            server_unknown_image_is_named,
        );
        $crate::contract_cases!(
            reports,
            $harness,
            reports_render_and_download,
            reports_refuse_unknown_jobs,
            reports_unknown_report_is_gone,
            results_and_result_images_round_trip,
        );
    };
}

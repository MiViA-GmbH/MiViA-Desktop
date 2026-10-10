use std::sync::Arc;

use async_trait::async_trait;
use mivia_ports::server::{
    MAX_STATUS_IDS, ServerApi, ServerError, ServerJobStatus, SubmitItem, UploadRequest,
};
use uuid::Uuid;

use super::{CaseResult, check, check_eq};

#[async_trait]
pub trait ServerHarness: Send + Sync {
    fn sut(&self) -> Arc<dyn ServerApi>;
    async fn finish_jobs(&self);
}

fn failed(error: &ServerError) -> String {
    format!("{error:?}")
}

async fn uploaded_image(server: &dyn ServerApi, bytes: &[u8]) -> Result<String, String> {
    let request = UploadRequest {
        file_name: "a.tif".to_owned(),
        bytes: bytes.to_vec(),
    };
    Ok(server
        .upload(request)
        .await
        .map_err(|e| failed(&e))?
        .image_id)
}

fn item(key: u128, image_id: &str) -> SubmitItem {
    SubmitItem {
        idempotency_key: Uuid::from_u128(key),
        image_id: image_id.to_owned(),
        model_id: "grain".to_owned(),
        customization_id: None,
    }
}

pub async fn server_same_content_is_one_image(harness: &impl ServerHarness) -> CaseResult {
    let server = harness.sut();
    let first = uploaded_image(server.as_ref(), b"same").await?;
    let again = server
        .upload(UploadRequest {
            file_name: "b.tif".to_owned(),
            bytes: b"same".to_vec(),
        })
        .await
        .map_err(|e| failed(&e))?;
    check_eq(
        &(again.image_id, again.duplicate),
        &(first, true),
        "second upload",
    )
}

pub async fn server_same_key_is_one_job(harness: &impl ServerHarness) -> CaseResult {
    let server = harness.sut();
    let image = uploaded_image(server.as_ref(), b"img").await?;
    let first = server
        .submit(vec![item(7, &image)])
        .await
        .map_err(|e| failed(&e))?;
    let again = server
        .submit(vec![item(7, &image)])
        .await
        .map_err(|e| failed(&e))?;
    let other = server
        .submit(vec![item(8, &image)])
        .await
        .map_err(|e| failed(&e))?;
    let job = |outcomes: &[mivia_ports::server::SubmitOutcome]| {
        outcomes.first().map(|outcome| outcome.job_id.clone())
    };
    check_eq(&job(&again), &job(&first), "resubmitted key")?;
    check(job(&other) != job(&first), "a new key reused a job")
}

pub async fn server_status_accepts_at_most_100_ids(harness: &impl ServerHarness) -> CaseResult {
    let ids = (0..=MAX_STATUS_IDS).map(|n| format!("job-{n}")).collect();
    let refused = harness.sut().job_statuses(ids).await;
    check(refused.is_err(), "101 status ids were accepted")
}

pub async fn server_jobs_finish_with_a_result(harness: &impl ServerHarness) -> CaseResult {
    let server = harness.sut();
    let image = uploaded_image(server.as_ref(), b"done").await?;
    let outcomes = server
        .submit(vec![item(9, &image)])
        .await
        .map_err(|e| failed(&e))?;
    let job_ids = outcomes
        .into_iter()
        .map(|outcome| outcome.job_id)
        .collect::<Vec<_>>();
    harness.finish_jobs().await;
    let statuses = server.job_statuses(job_ids).await.map_err(|e| failed(&e))?;
    let done = statuses
        .iter()
        .all(|status| status.status == ServerJobStatus::New && status.result_id.is_some());
    check(done && statuses.len() == 1, "finished job has no result")
}

#[macro_export]
macro_rules! server_contract_tests {
    ($harness:expr) => {
        $crate::contract_cases!(
            server,
            $harness,
            server_same_content_is_one_image,
            server_same_key_is_one_job,
            server_status_accepts_at_most_100_ids,
            server_jobs_finish_with_a_result,
        );
    };
}

use std::path::Path;
use std::time::SystemTime;

use mivia_domain::ids::ModelChoice;
use mivia_ports::server::{
    CatalogueModel, Download, FindJobs, ServerApi, ServerJob, ServerJobStatus, SubmitRequest,
    UploadRequest,
};
use reqwest::header::AUTHORIZATION;
use serde_json::{Value, json};

use super::{Bytes, Collect, DEADLINE, Outcome, PAIRS, POLL, Smoke, check, fresh_uuid, say};

async fn upload(smoke: &Smoke, samples: &Path, n: usize, stem: &str) -> Outcome<String> {
    let bytes = std::fs::read(samples.join(format!("{stem}.png")))?;
    let file_name = format!("image-{n}.png");
    let request = UploadRequest {
        file_name: file_name.clone(),
        len: u64::try_from(bytes.len())?,
        source: Box::new(Bytes(Some(bytes.clone()))),
    };
    let image = smoke
        .api
        .upload(request)
        .await
        .map_err(|e| format!("upload {stem}: {e:?}"))?;
    check(!image.image_id.is_empty(), &format!("upload {file_name}"))?;
    let client = smoke.clients.load().upload.clone();
    let part = reqwest::multipart::Part::bytes(bytes).file_name(file_name);
    let request = client
        .post(format!("{}/api/image", smoke.base))
        .header(AUTHORIZATION, smoke.auth()?)
        .multipart(reqwest::multipart::Form::new().part("files", part));
    let (_, body) = smoke.raw("ImageController_uploadImages", request).await?;
    let echoed: Value = serde_json::from_slice(&body)?;
    check(
        echoed.pointer("/0/duplicate") == Some(&Value::Bool(true)),
        "the same content uploads as a duplicate",
    )?;
    Ok(image.image_id)
}

fn finished(job: &ServerJob) -> bool {
    !matches!(
        job.status,
        ServerJobStatus::Pending | ServerJobStatus::Unknown(_)
    )
}

pub(crate) async fn compute(
    smoke: &Smoke,
    samples: &Path,
    catalogue: &[CatalogueModel],
) -> Outcome<Vec<ServerJob>> {
    let mut jobs = Vec::new();
    for (n, (stem, model)) in PAIRS.iter().enumerate() {
        let Some(found) = catalogue.iter().find(|m| m.internal_name == *model) else {
            say(&format!("      skip {stem}: no model {model}"));
            continue;
        };
        let image_id = upload(smoke, samples, n + 1, stem).await?;
        let choice = ModelChoice {
            model_id: found.model_id.clone(),
            customization_id: None,
        };
        let submitted = smoke
            .api
            .submit(SubmitRequest {
                key: fresh_uuid(),
                choice: choice.clone(),
                image_ids: vec![image_id.clone()],
                force: false,
            })
            .await
            .map_err(|e| format!("submit {model}: {e:?}"))?;
        check(submitted.len() == 1, &format!("submit {model}"))?;
        check(
            submitted
                .iter()
                .all(|job| job.status != ServerJobStatus::Cached || job.result_id.is_some()),
            &format!("cached {model} job carries its result id"),
        )?;
        if smoke.recorder.recording() {
            let body =
                json!({ "modelId": found.model_id, "imageIds": [image_id], "source": "API" });
            smoke
                .post_json("JobsController_createJobs", "/api/jobs", &body)
                .await?;
        }
        jobs.extend(submitted);
        let listed = smoke
            .api
            .find_jobs(FindJobs { choice, page: 1 })
            .await
            .map_err(|e| format!("find jobs {model}: {e:?}"))?;
        check(!listed.is_empty(), &format!("find jobs {model}"))?;
    }
    poll(smoke, jobs).await
}

async fn poll(smoke: &Smoke, mut jobs: Vec<ServerJob>) -> Outcome<Vec<ServerJob>> {
    let ids: Vec<String> = jobs.iter().map(|job| job.job_id.clone()).collect();
    let started = SystemTime::now();
    while !jobs.iter().all(finished) {
        if started.elapsed().unwrap_or_default() > DEADLINE {
            return Err("jobs did not finish within 15 minutes".into());
        }
        tokio::time::sleep(POLL).await;
        jobs = smoke
            .api
            .job_statuses(ids.clone())
            .await
            .map_err(|e| format!("status: {e:?}"))?;
    }
    let failed = jobs
        .iter()
        .filter(|job| job.status == ServerJobStatus::Failed)
        .count();
    let states: Vec<_> = jobs.iter().map(|job| &job.status).collect();
    check(
        failed == 0,
        &format!("{} jobs finished: {states:?}", jobs.len()),
    )?;
    let joined = ids.join(",");
    smoke
        .get(
            "JobsV2Controller_getJobsStatus",
            &format!("/api/v2/jobs/status?ids={joined}"),
        )
        .await?;
    let first = jobs.first().ok_or("no job")?;
    check(
        smoke
            .api
            .job_exists(&first.job_id)
            .await
            .map_err(|e| format!("exists: {e:?}"))?,
        "job exists",
    )?;
    let detail = smoke
        .get(
            "JobsV2Controller_getJobResults",
            &format!("/api/v2/jobs/{}", first.job_id),
        )
        .await?;
    check(
        serde_json::from_slice::<mivia_api::types::JobV2Detail>(&detail).is_ok(),
        "job results parse",
    )?;
    result_image(smoke, &first.job_id).await?;
    smoke
        .get(
            "JobsV2Controller_getJobsLight",
            "/api/v2/jobs?page=1&pageSize=5&sortBy=createdAt&sortOrder=desc&source=API",
        )
        .await?;
    Ok(jobs)
}

async fn result_image(smoke: &Smoke, job_id: &str) -> Outcome<()> {
    let results = smoke
        .api
        .job_results(job_id)
        .await
        .map_err(|e| format!("results: {e:?}"))?
        .ok_or("no results")?;
    let items: Vec<Value> = serde_json::from_str(&results)?;
    check(!items.is_empty(), "job results array")?;
    let name = items
        .iter()
        .find(|item| item["type"] == "image")
        .and_then(|item| item["filename"].as_str());
    if let Some(name) = name {
        let download = smoke
            .api
            .download_result_image(job_id, name, Box::new(Collect(Vec::new())))
            .await
            .map_err(|e| format!("result image: {e:?}"))?;
        check(
            matches!(download, Download::Staged(staged) if staged.size > 0),
            "result image download",
        )?;
    }
    Ok(())
}

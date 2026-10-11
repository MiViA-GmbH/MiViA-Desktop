use mivia_domain::naming::Language;
use mivia_domain::output::Format;
use mivia_ports::server::{Download, ReportProgress, ReportRequest, ServerApi, ServerError};

use super::server::{ServerHarness, image, submit};
use super::{CaseResult, check, check_eq};
use crate::fs::MemorySink;
use crate::server::sim_id;

const STATUS_ROUNDS: usize = 10;

fn request(format: Format, job_ids: Vec<String>) -> ReportRequest {
    ReportRequest {
        format,
        job_ids,
        timezone: "Europe/Berlin".to_owned(),
        include_images: false,
        language: Language::De,
        extras: None,
    }
}

fn failed(error: &ServerError) -> String {
    format!("{error:?}")
}

async fn finished_job(harness: &impl ServerHarness, content: &[u8]) -> Result<String, String> {
    let server = harness.sut();
    let id = image(server.as_ref(), content).await?;
    let jobs = submit(server.as_ref(), &[&id]).await?;
    harness.finish_jobs().await;
    let ids: Vec<String> = jobs.into_iter().map(|job| job.job_id).collect();
    server
        .job_statuses(ids.clone())
        .await
        .map_err(|e| failed(&e))?;
    ids.into_iter().next().ok_or_else(|| "no job".to_owned())
}

async fn rendered(server: &dyn ServerApi, report: &str) -> CaseResult {
    for _ in 0..STATUS_ROUNDS {
        match server.report_status(report).await.map_err(|e| failed(&e))? {
            ReportProgress::Done => return Ok(()),
            ReportProgress::Pending => {}
            other @ (ReportProgress::Cancelled | ReportProgress::Failed { .. }) => {
                return Err(format!("report ended {other:?}"));
            }
        }
    }
    Err("report never finished".to_owned())
}

pub async fn reports_render_and_download(harness: &impl ServerHarness) -> CaseResult {
    let job = finished_job(harness, b"reported").await?;
    let server = harness.sut();
    for (format, magic) in [(Format::Pdf, &b"%PDF-"[..]), (Format::Csv, b"PK\x03\x04")] {
        let report = server
            .request_report(request(format, vec![job.clone()]))
            .await
            .map_err(|e| failed(&e))?;
        rendered(server.as_ref(), &report).await?;
        let download = server
            .download_report(&report, Box::new(MemorySink::new()))
            .await
            .map_err(|e| failed(&e))?;
        let Download::Staged(staged) = download else {
            return Err("download expired".to_owned());
        };
        check(staged.head.starts_with(magic), "report bytes")?;
        check(staged.size > 4, "report size")?;
    }
    Ok(())
}

pub async fn reports_refuse_unknown_jobs(harness: &impl ServerHarness) -> CaseResult {
    let outcome = harness
        .sut()
        .request_report(request(Format::Pdf, vec![sim_id(2, 998)]))
        .await;
    check(
        matches!(outcome, Err(ServerError::Forbidden { .. })),
        &format!("unknown job: {outcome:?}"),
    )
}

pub async fn reports_unknown_report_is_gone(harness: &impl ServerHarness) -> CaseResult {
    let outcome = harness.sut().report_status(&sim_id(4, 997)).await;
    check_eq(&outcome, &Err(ServerError::Gone), "unknown report")
}

pub async fn results_and_result_images_round_trip(harness: &impl ServerHarness) -> CaseResult {
    let job = finished_job(harness, b"pictured").await?;
    let server = harness.sut();
    let results = r#"[{"filename":"r.png","type":"image"}]"#;
    harness
        .seed_results(&job, results, ("r.png", b"\x89PNG-body"))
        .await;
    let found = server.job_results(&job).await.map_err(|e| failed(&e))?;
    check_eq(&found.as_deref(), &Some(results), "results")?;
    let download = server
        .download_result_image(&job, "r.png", Box::new(MemorySink::new()))
        .await
        .map_err(|e| failed(&e))?;
    let Download::Staged(staged) = download else {
        return Err("image expired".to_owned());
    };
    check_eq(&staged.size, &9, "image size")?;
    let missing = server
        .download_result_image(&job, "other.png", Box::new(MemorySink::new()))
        .await;
    check_eq(&missing, &Ok(Download::Expired), "unknown image")
}

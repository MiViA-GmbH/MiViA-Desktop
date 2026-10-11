use std::time::SystemTime;

use mivia_domain::naming::Language;
use mivia_domain::output::Format;
use mivia_domain::report::OmittedReason;
use mivia_ports::server::{
    Download, ReportExtras, ReportProgress, ReportRequest, ServerApi, ServerJob,
};
use serde_json::json;

use super::{Collect, DEADLINE, Outcome, POLL, Smoke, check};

fn report_request(format: Format, jobs: &[ServerJob]) -> ReportRequest {
    let labels = (1..=jobs.len()).map(|n| format!("image-{n}")).collect();
    ReportRequest {
        format,
        job_ids: jobs.iter().map(|job| job.job_id.clone()).collect(),
        timezone: "Europe/Berlin".to_owned(),
        include_images: false,
        language: Language::En,
        extras: Some(ReportExtras {
            labels,
            part: Some((1, 1)),
            omitted: vec![("image-9".to_owned(), OmittedReason::Failed)],
            replaces: None,
            decimal_comma: format == Format::Csv,
        }),
    }
}

async fn wait_report(smoke: &Smoke, id: &str) -> Outcome<()> {
    let started = SystemTime::now();
    loop {
        match smoke
            .api
            .report_status(id)
            .await
            .map_err(|e| format!("report status: {e:?}"))?
        {
            ReportProgress::Done => return Ok(()),
            ReportProgress::Pending => {}
            other @ (ReportProgress::Cancelled | ReportProgress::Failed { .. }) => {
                return Err(format!("report {id}: {other:?}").into());
            }
        }
        if started.elapsed().unwrap_or_default() > DEADLINE {
            return Err("report did not finish within 15 minutes".into());
        }
        tokio::time::sleep(POLL).await;
    }
}

pub(crate) async fn reports(smoke: &Smoke, jobs: &[ServerJob]) -> Outcome<()> {
    for (format, magic) in [(Format::Pdf, &b"%PDF"[..]), (Format::Csv, &b"PK"[..])] {
        let id = smoke
            .api
            .request_report(report_request(format, jobs))
            .await
            .map_err(|e| format!("report {format:?}: {e:?}"))?;
        wait_report(smoke, &id).await?;
        let download = smoke
            .api
            .download_report(&id, Box::new(Collect(Vec::new())))
            .await
            .map_err(|e| format!("download {format:?}: {e:?}"))?;
        let ok = matches!(&download, Download::Staged(staged) if staged.head.starts_with(magic));
        check(
            ok,
            &format!("report v2 {format:?} requested, done, downloaded"),
        )?;
    }
    if !smoke.recorder.recording() {
        return Ok(());
    }
    let body = json!({
        "jobsIds": jobs.iter().map(|job| job.job_id.clone()).collect::<Vec<_>>(),
        "timezone": "Europe/Berlin",
        "includeImages": false,
        "supersede": false,
        "summary": true,
    });
    let created = smoke
        .post_json("ReportingController_createPdf", "/api/reports/pdf", &body)
        .await?;
    let created: mivia_api::ReportCreated = serde_json::from_slice(&created)?;
    wait_report(smoke, &created.report_id).await?;
    smoke
        .get(
            "ReportingController_getReport",
            &format!("/api/reports/{}", created.report_id),
        )
        .await?;
    smoke
        .get(
            "ReportingController_downloadReport",
            &format!("/api/reports/{}/download", created.report_id),
        )
        .await?;
    Ok(())
}

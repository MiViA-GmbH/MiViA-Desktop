use std::io::Write as _;

use mivia_ports::ipc::support::ReportTrigger;
use mivia_ports::server::{ProblemReportForm, ServerApi};
use mivia_ports::time::iso_utc;
use reqwest::header::{AUTHORIZATION, CONTENT_ENCODING, CONTENT_TYPE, HeaderValue};
use serde_json::json;

use super::{Outcome, Smoke, check, fresh_uuid, now};

fn bundle() -> Outcome<Vec<u8>> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("smoke.txt", zip::write::SimpleFileOptions::default())?;
    zip.write_all(b"workstation smoke test\n")?;
    Ok(zip.finish()?.into_inner())
}

fn form(session: &str) -> ProblemReportForm {
    ProblemReportForm {
        report_id: fresh_uuid().to_string(),
        trigger: ReportTrigger::Manual,
        signature: Some("smoke".to_owned()),
        description: Some("Workstation smoke test".to_owned()),
        app_version: "2.0.0".to_owned(),
        os: "Windows 10.0.26200".to_owned(),
        machine_id: "unknown".to_owned(),
        install_id: fresh_uuid().to_string(),
        session_id: session.to_owned(),
    }
}

pub(crate) async fn problem_report(smoke: &Smoke, session: &str) -> Outcome<()> {
    smoke
        .api
        .send_problem_report(&form(session), bundle()?)
        .await
        .map_err(|e| format!("problem report: {e:?}"))?;
    check(true, "problem report sent")?;
    if !smoke.recorder.recording() {
        return Ok(());
    }
    let report = form(session);
    let zip = reqwest::header::HeaderMap::from_iter([(
        CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    )]);
    let part = reqwest::multipart::Part::bytes(bundle()?)
        .file_name(format!("MiViA-report-{}.zip", report.report_id))
        .headers(zip);
    let form = reqwest::multipart::Form::new()
        .text("reportId", report.report_id)
        .text("trigger", "manual")
        .text("appVersion", report.app_version)
        .text("os", report.os)
        .text("machineId", report.machine_id)
        .text("installId", report.install_id)
        .part("bundle", part);
    let client = smoke.clients.load().upload.clone();
    let request = client
        .post(format!(
            "{}/api/client-diagnostics/issue-reports",
            smoke.base
        ))
        .header(AUTHORIZATION, smoke.auth()?)
        .header("x-client-session-id", session)
        .multipart(form);
    smoke
        .raw("ClientDiagnosticsController_createIssueReport", request)
        .await?;
    Ok(())
}

fn batch(session: &str) -> Outcome<Vec<u8>> {
    let body = json!({
        "schema": 1,
        "batchId": fresh_uuid().to_string(),
        "sentAt": iso_utc(now()),
        "installId": fresh_uuid().to_string(),
        "machineId": "unknown",
        "sessionId": session,
        "resource": { "appVersion": "2.0.0", "arch": "x64", "channel": "stable", "uiLang": "en" },
        "events": [{
            "seq": 1,
            "ts": iso_utc(now()),
            "name": "session.start",
            "v": 1,
            "category": "diagnostics",
            "accountMatch": true,
            "fields": { "state": "user", "first_run": false },
        }],
    });
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(body.to_string().as_bytes())?;
    Ok(encoder.finish()?)
}

pub(crate) async fn telemetry(smoke: &Smoke, session: &str) -> Outcome<()> {
    let ack = smoke
        .api
        .send_telemetry(session, batch(session)?)
        .await
        .map_err(|e| format!("telemetry: {e:?}"))?;
    check(
        ack.accepted == 1 && ack.rejected.is_empty(),
        "telemetry batch accepted",
    )?;
    if !smoke.recorder.recording() {
        return Ok(());
    }
    let client = smoke.clients.load().api.clone();
    let request = client
        .post(format!("{}/api/client-diagnostics/telemetry", smoke.base))
        .header(AUTHORIZATION, smoke.auth()?)
        .header(CONTENT_TYPE, "application/json")
        .header(CONTENT_ENCODING, "gzip")
        .header("x-client-session-id", session)
        .body(batch(session)?);
    smoke
        .raw("ClientTelemetryController_receive", request)
        .await?;
    Ok(())
}

use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use mivia_testkit::domain::naming::Language;
use mivia_testkit::domain::output::Format;
use mivia_testkit::domain::report::OmittedReason;
use mivia_testkit::fs::staged;
use mivia_testkit::ports::fs::{ByteSink, FsError, Staged};
use mivia_testkit::ports::server::{
    Download, ReportExtras, ReportProgress, ReportRequest, ServerApi,
};
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::json;

use crate::Stub;
use crate::api::denied;
use crate::errors::response;

pub(crate) fn routes() -> Router<Stub> {
    Router::new()
        .route("/api/reports/{id}", get(status).post(create))
        .route("/api/reports/{id}/download", get(link))
        .route("/__stub/files/{id}", get(file))
}

#[derive(Deserialize)]
struct Part {
    number: u32,
    of: u32,
}

#[derive(Deserialize)]
struct Omitted {
    label: String,
    reason: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateReport {
    jobs_ids: Vec<String>,
    timezone: Option<String>,
    include_images: Option<bool>,
    supersede: Option<bool>,
    labels: Option<Vec<String>>,
    part: Option<Part>,
    omitted: Option<Vec<Omitted>>,
    replaces: Option<String>,
    decimal_separator: Option<String>,
}

fn extras(body: &CreateReport) -> Option<ReportExtras> {
    let kept = body.supersede == Some(false);
    (kept || body.labels.is_some()).then(|| ReportExtras {
        labels: body.labels.clone().unwrap_or_default(),
        part: body.part.as_ref().map(|part| (part.number, part.of)),
        omitted: body
            .omitted
            .iter()
            .flatten()
            .filter_map(|o| OmittedReason::from_code(&o.reason).map(|r| (o.label.clone(), r)))
            .collect(),
        replaces: body.replaces.clone(),
        decimal_comma: body.decimal_separator.as_deref() == Some("comma"),
    })
}

async fn create(
    State(stub): State<Stub>,
    headers: HeaderMap,
    Path(format): Path<String>,
    Json(body): Json<CreateReport>,
) -> Response {
    if let Some(denied) = denied(&stub, &headers) {
        return denied;
    }
    let Some(format) = Format::from_code(&format) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let language = headers
        .get(header::ACCEPT_LANGUAGE)
        .and_then(|v| v.to_str().ok())
        .and_then(Language::from_code)
        .unwrap_or(Language::En);
    let request = ReportRequest {
        format,
        job_ids: body.jobs_ids.clone(),
        timezone: body.timezone.clone().unwrap_or_else(|| "UTC".to_owned()),
        include_images: body.include_images.unwrap_or(true),
        language,
        extras: extras(&body),
    };
    match stub.engine.request_report(request).await {
        Ok(id) => (
            StatusCode::ACCEPTED,
            Json(json!({ "reportId": id, "status": "PENDING" })),
        )
            .into_response(),
        Err(error) => response(&error),
    }
}

async fn status(State(stub): State<Stub>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    if let Some(denied) = denied(&stub, &headers) {
        return denied;
    }
    let (status, code) = match stub.engine.report_status(&id).await {
        Ok(ReportProgress::Pending) => ("PENDING", None),
        Ok(ReportProgress::Done) => ("DONE", None),
        Ok(ReportProgress::Cancelled) => ("CANCELLED", None),
        Ok(ReportProgress::Failed { too_large: true }) => ("FAILED", Some("REPORT_TOO_LARGE")),
        Ok(ReportProgress::Failed { too_large: false }) => ("FAILED", Some("REPORT_FAILED")),
        Err(error) => return response(&error),
    };
    Json(json!({ "id": id, "status": status, "errorCode": code, "createdAt": "2026-10-09T10:00:00.000Z" }))
        .into_response()
}

#[derive(Default)]
struct Collect(Arc<Mutex<Vec<u8>>>);

#[async_trait]
impl ByteSink for Collect {
    async fn write(&mut self, chunk: Vec<u8>) -> Result<(), FsError> {
        self.0.lock().extend(chunk);
        Ok(())
    }

    async fn finish(self: Box<Self>) -> Result<Staged, FsError> {
        let bytes = self.0.lock();
        Ok(staged(&bytes))
    }
}

async fn link(State(stub): State<Stub>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    if let Some(denied) = denied(&stub, &headers) {
        return denied;
    }
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let sink = Collect(Arc::clone(&bytes));
    match stub.engine.download_report(&id, Box::new(sink)).await {
        Ok(Download::Expired) => Json(json!({ "status": "expired" })).into_response(),
        Ok(Download::Staged(_)) => {
            let host = headers
                .get(header::HOST)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("127.0.0.1");
            let body = std::mem::take(&mut *bytes.lock());
            stub.files.lock().insert(id.clone(), body);
            Json(json!({ "url": format!("http://{host}/__stub/files/{id}") })).into_response()
        }
        Err(error) => response(&error),
    }
}

async fn file(State(stub): State<Stub>, Path(id): Path<String>) -> Response {
    let found = stub.files.lock().remove(&id);
    match found {
        Some(bytes) => bytes.into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

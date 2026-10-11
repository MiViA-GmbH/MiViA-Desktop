use std::collections::BTreeMap;
use std::io::Read;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Multipart, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use mivia_testkit::ports::server::{ProblemReportForm, ServerApi};
use serde_json::{Value, json};

use crate::Stub;
use crate::api::denied;
use crate::errors::response;

const BODY_LIMIT: usize = 11 * 1024 * 1024;

pub(crate) fn routes() -> Router<Stub> {
    Router::new()
        .route(
            "/api/client-diagnostics/issue-reports",
            post(create).layer(DefaultBodyLimit::max(BODY_LIMIT)),
        )
        .route("/api/client-diagnostics/telemetry", post(telemetry))
        .route("/__stub/telemetry", get(received))
}

const DECODED_LIMIT: u64 = 100_000;
const MAX_EVENTS: usize = 500;

fn decoded(body: &[u8]) -> Option<Value> {
    let mut text = Vec::new();
    flate2::read::GzDecoder::new(body)
        .take(DECODED_LIMIT + 1)
        .read_to_end(&mut text)
        .ok()?;
    serde_json::from_slice(&text).ok()
}

fn telemetry_invalid() -> Response {
    let payload = json!({ "statusCode": 400, "code": "TELEMETRY_INVALID", "message": "Invalid" });
    (StatusCode::BAD_REQUEST, Json(payload)).into_response()
}

async fn telemetry(State(stub): State<Stub>, headers: HeaderMap, body: Bytes) -> Response {
    if let Some(denied) = denied(&stub, &headers) {
        return denied;
    }
    if headers.get(header::CONTENT_ENCODING) != Some(&HeaderValue::from_static("gzip")) {
        return telemetry_invalid();
    }
    let events = decoded(&body)
        .and_then(|batch| batch.get("events").and_then(Value::as_array).map(Vec::len))
        .filter(|count| (1..=MAX_EVENTS).contains(count));
    let Some(events) = events else {
        return telemetry_invalid();
    };
    let session_id = headers
        .get("x-client-session-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    match stub.engine.send_telemetry(session_id, body.to_vec()).await {
        Ok(_) => {
            let mut response = Json(json!({ "accepted": events, "rejected": [] })).into_response();
            response.headers_mut().insert(
                "x-telemetry-min-interval-s",
                HeaderValue::from_static("300"),
            );
            response
        }
        Err(error) => response(&error),
    }
}

fn form(fields: &BTreeMap<String, String>, session_id: String) -> Option<ProblemReportForm> {
    let text = |name: &str| fields.get(name).cloned();
    Some(ProblemReportForm {
        report_id: text("reportId")?,
        trigger: serde_json::from_value(Value::String(text("trigger")?)).ok()?,
        signature: text("signature"),
        description: text("description"),
        app_version: text("appVersion")?,
        os: text("os")?,
        machine_id: text("machineId")?,
        install_id: text("installId")?,
        session_id,
    })
}

fn invalid() -> Response {
    let payload =
        json!({ "statusCode": 400, "code": "ISSUE_REPORT_INVALID", "message": "Invalid" });
    (StatusCode::BAD_REQUEST, Json(payload)).into_response()
}

async fn parts(multipart: &mut Multipart) -> Option<(BTreeMap<String, String>, Option<Vec<u8>>)> {
    let mut fields = BTreeMap::new();
    let mut bundle = None;
    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or_default().to_owned();
        let zip = field.content_type() == Some("application/zip");
        let bytes = field.bytes().await.ok()?;
        if name == "bundle" && zip {
            bundle = Some(bytes.to_vec());
        } else {
            fields.insert(name, String::from_utf8_lossy(&bytes).into_owned());
        }
    }
    Some((fields, bundle))
}

async fn create(
    State(stub): State<Stub>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Response {
    if let Some(denied) = denied(&stub, &headers) {
        return denied;
    }
    let Some((fields, bundle)) = parts(&mut multipart).await else {
        return invalid();
    };
    let session_id = headers
        .get("x-client-session-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let (Some(form), Some(bundle)) = (form(&fields, session_id), bundle) else {
        return invalid();
    };
    let repeated = stub
        .engine
        .problem_reports()
        .iter()
        .any(|seen| seen.report_id == form.report_id);
    match stub.engine.send_problem_report(&form, bundle).await {
        Ok(()) => {
            let status = if repeated {
                StatusCode::OK
            } else {
                StatusCode::CREATED
            };
            let body =
                json!({ "reportId": form.report_id, "receivedAt": "2026-01-01T00:00:00.000Z" });
            (status, Json(body)).into_response()
        }
        Err(error) => response(&error),
    }
}

async fn received(State(stub): State<Stub>) -> Response {
    let batches: Vec<Value> = stub
        .engine
        .telemetry_batches()
        .iter()
        .filter_map(|(_, body)| decoded(body))
        .collect();
    Json(batches).into_response()
}

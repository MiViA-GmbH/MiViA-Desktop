use std::collections::BTreeMap;

use axum::extract::{DefaultBodyLimit, Multipart, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use mivia_testkit::ports::server::{ProblemReportForm, ServerApi};
use serde_json::{Value, json};

use crate::Stub;
use crate::api::denied;
use crate::errors::response;

const BODY_LIMIT: usize = 11 * 1024 * 1024;

pub(crate) fn routes() -> Router<Stub> {
    Router::new().route(
        "/api/client-diagnostics/issue-reports",
        post(create).layer(DefaultBodyLimit::max(BODY_LIMIT)),
    )
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
        let bytes = field.bytes().await.ok()?;
        if name == "bundle" {
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
